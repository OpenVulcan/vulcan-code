//! Project summary models shared by the task tree and persistence layers.
//! 供任务树与持久化层共享的项目摘要模型。

use crate::SessionId;
use uuid::Uuid;

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
}

impl TaskProjectSummary {
    /// Creates one project node with explicit metadata and child-session bindings.
    /// 使用明确元数据与子会话绑定创建一个项目节点。
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
        }
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
