//! Project summary models shared by the task tree and persistence layers.
//! 供任务树与持久化层共享的项目摘要模型。

use crate::SessionId;
use uuid::Uuid;

/// How the sessions of one project are ordered in the sidebar; the project row persists the choice.
/// 侧栏中一个项目的会话排列方式；该选择由项目行持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectSessionSortMode {
    /// Newest request first; the list cannot be dragged. This is the default for unconfigured projects.
    /// 最近请求优先；列表不可拖动。未配置的项目使用此默认值。
    #[default]
    LastRequest,
    /// User-arranged order; sessions never arranged follow newest creation first above arranged ones.
    /// 用户手动排列；从未排列过的会话按创建时间从新到旧排在已排列会话之前。
    Manual,
}

impl ProjectSessionSortMode {
    /// Every mode in declaration order, the single source of the stored value set.
    /// 按声明顺序列出的全部模式，是存储取值集合的唯一来源。
    pub const ALL: [Self; 2] = [Self::LastRequest, Self::Manual];

    /// Returns the stable stored value of the mode.
    /// 返回该模式稳定的存储值。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LastRequest => "last_request",
            Self::Manual => "manual",
        }
    }

    /// Parses one stored value; returns `None` for any value outside `ALL`.
    /// 解析一个存储值；不属于 `ALL` 的值返回 `None`。
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == value)
    }
}

/// Lightweight project node shown by the main task-drawer tree.
/// 主任务抽屉树中展示的轻量级项目节点。
#[derive(Debug, Clone)]
pub struct TaskProjectSummary {
    /// Stable project identifier used by the tree state.
    /// 供树状态使用的稳定项目标识。
    pub id: String,
    /// Project display title.
    /// 项目展示标题。
    pub title: String,
    /// Absolute project path used when opening the directory.
    /// 用于打开目录的项目绝对路径。
    pub path: String,
    /// Whether the project is currently pinned in the tree.
    /// 当前项目是否已在树中置顶。
    pub pinned: bool,
    /// Stable order used inside the current project section.
    /// 当前项目所在分区内部使用的稳定顺序值。
    pub order: usize,
    /// Optional project icon pack selected by the user.
    /// 用户选择的可选项目图标包。
    pub icon_pack: Option<String>,
    /// Optional project icon key selected by the user.
    /// 用户选择的可选项目图标键。
    pub icon_key: Option<String>,
    /// Session identifiers nested under the project node.
    /// 挂载在项目节点下的会话标识集合。
    pub session_ids: Vec<SessionId>,
    /// How the project's sessions are ordered in the sidebar.
    /// 项目会话在侧栏中的排列方式。
    pub session_sort_mode: ProjectSessionSortMode,
}

impl TaskProjectSummary {
    /// Creates one project node with explicit metadata and child-session bindings.
    /// 使用明确元数据与子会话绑定创建一个项目节点。
    #[expect(
        clippy::too_many_arguments,
        reason = "The constructor mirrors the persisted project summary shape used by seed and store callers."
    )]
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        path: impl Into<String>,
        pinned: bool,
        order: usize,
        icon_pack: Option<String>,
        icon_key: Option<String>,
        session_ids: Vec<SessionId>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            path: path.into(),
            pinned,
            order,
            icon_pack,
            icon_key,
            session_ids,
            session_sort_mode: ProjectSessionSortMode::default(),
        }
    }

    /// Records the persisted session sort mode of the project.
    /// 记录项目已持久化的会话排列方式。
    pub fn with_session_sort_mode(mut self, session_sort_mode: ProjectSessionSortMode) -> Self {
        self.session_sort_mode = session_sort_mode;
        self
    }

    /// Builds seed project-tree data for the workbench task drawer prototype.
    /// 为工作台任务抽屉原型构建种子项目树数据。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                "vulcan-luaskills",
                "LuaSkills",
                r"D:\projects\LuaSkills",
                false,
                0,
                None,
                None,
                vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            ),
            Self::new(
                "vulcan-memory-mesh",
                "VulcanMemoryMesh",
                r"D:\projects\VulcanMemoryMesh",
                false,
                1,
                None,
                None,
                vec![Uuid::from_u128(3), Uuid::from_u128(4)],
            ),
            Self::new(
                "vulcan-plugins-opencode",
                "vulcan-plugins-opencode",
                r"D:\projects\vulcan-plugins-opencode",
                false,
                2,
                None,
                None,
                vec![],
            ),
        ]
    }
}
