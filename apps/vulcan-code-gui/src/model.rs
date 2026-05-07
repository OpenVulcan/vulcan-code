//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use std::fmt;

/// Execution modes for AI coding tasks.
/// AI 编码任务的执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Generates a plan before modification.
    /// 修改前先生成计划。
    Plan,
    /// Applies patches directly to target files.
    /// 直接向目标文件应用补丁。
    Patch,
    /// Reviews code and produces findings.
    /// 以评审方式输出问题与结论。
    Review,
}

impl ExecutionMode {
    /// All execution modes exposed in the radio group.
    /// 单选组件中暴露的全部执行模式。
    pub const ALL: [Self; 3] = [Self::Plan, Self::Patch, Self::Review];
}

impl fmt::Display for ExecutionMode {
    /// Renders a user-facing execution mode label.
    /// 渲染面向用户的执行模式标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Plan => "Plan First",
            Self::Patch => "Patch Directly",
            Self::Review => "Review Mode",
        };

        formatter.write_str(label)
    }
}

/// Runtime status of a coding session.
/// 编码会话的运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is waiting for execution.
    /// 会话等待执行中。
    Draft,
    /// Session is currently being processed.
    /// 会话正在处理中。
    Running,
    /// Session is blocked by a dependency or decision.
    /// 会话被依赖或决策阻塞。
    Blocked,
    /// Session has completed its latest run.
    /// 会话已完成最近一次执行。
    Completed,
}

impl SessionState {
    /// Returns a concise status label for the UI.
    /// 返回界面使用的精简状态标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Running => "Running",
            Self::Blocked => "Blocked",
            Self::Completed => "Completed",
        }
    }
}

/// Navigation pane descriptors used by the pane grid.
/// 供分栏网格使用的面板描述类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    /// Project navigation and sessions.
    /// 项目导航与会话区域。
    Navigator,
    /// Central prompt and editor workbench.
    /// 中央提示词与编辑器工作台。
    Workbench,
    /// Runtime controls and diagnostics.
    /// 运行控制与诊断区域。
    Inspector,
}

impl PaneKind {
    /// Returns the pane title shown in the title bar.
    /// 返回标题栏中展示的面板标题。
    pub const fn title(self) -> &'static str {
        match self {
            Self::Navigator => "Project Navigator",
            Self::Workbench => "Agent Workbench",
            Self::Inspector => "Runtime Inspector",
        }
    }

    /// Returns the pane subtitle shown in the title bar.
    /// 返回标题栏中展示的面板副标题。
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Navigator => "workspace + sessions",
            Self::Workbench => "prompt + patch editor",
            Self::Inspector => "controls + telemetry",
        }
    }
}

/// Lightweight summary of a coding session.
/// 编码会话的轻量级摘要信息。
#[derive(Debug, Clone)]
pub struct SessionSummary {
    /// Stable session identifier for selection.
    /// 用于选择状态的稳定会话标识。
    pub id: usize,
    /// Session display title.
    /// 会话展示标题。
    pub title: String,
    /// Short current focus summary.
    /// 当前聚焦点的简要说明。
    pub focus: String,
    /// Current lifecycle state.
    /// 当前生命周期状态。
    pub status: SessionState,
    /// Number of files expected to change.
    /// 预计会变更的文件数量。
    pub changed_files: u8,
    /// Number of queued execution steps.
    /// 已排队的执行步骤数量。
    pub queued_steps: u8,
    /// Relative last-updated label shown in compact tree views.
    /// 展示在紧凑树视图中的相对更新时间标签。
    pub last_updated_label: String,
}

impl SessionSummary {
    /// Creates a new session summary with explicit metadata.
    /// 使用明确元数据创建新的会话摘要。
    pub fn new(
        id: usize,
        title: impl Into<String>,
        focus: impl Into<String>,
        status: SessionState,
        changed_files: u8,
        queued_steps: u8,
        last_updated_label: impl Into<String>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            focus: focus.into(),
            status,
            changed_files,
            queued_steps,
            last_updated_label: last_updated_label.into(),
        }
    }

    /// Builds seed session data for the first prototype window.
    /// 为首版原型窗口构建种子会话数据。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                1,
                "重命名中枢系统",
                "收敛工作台命名与主入口语义",
                SessionState::Running,
                7,
                4,
                "1天前",
            ),
            Self::new(
                2,
                "评估 LuaSkills 会话声明机制",
                "梳理技能、工具与角色接入策略",
                SessionState::Completed,
                3,
                2,
                "3天前",
            ),
            Self::new(
                3,
                "检查构建脚本异常相关问题",
                "核对桌面窗口、打包与启动细节",
                SessionState::Blocked,
                2,
                1,
                "3周前",
            ),
            Self::new(
                4,
                "为 opencode.json 引用安装",
                "梳理供应商配置读取与安装体验",
                SessionState::Draft,
                1,
                2,
                "26天前",
            ),
        ]
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
    /// Session identifiers nested under the project node.
    /// 挂载在项目节点下的会话标识集合。
    pub session_ids: Vec<usize>,
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
        session_ids: Vec<usize>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            path: path.into(),
            pinned,
            order,
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
                "D:/projects/LuaSkills",
                false,
                0,
                vec![1, 2],
            ),
            Self::new(
                "vulcan-memory-mesh",
                "VulcanMemoryMesh",
                "D:/projects/VulcanMemoryMesh",
                false,
                1,
                vec![3, 4],
            ),
            Self::new(
                "vulcan-plugins-opencode",
                "vulcan-plugins-opencode",
                "D:/projects/vulcan-plugins-opencode",
                false,
                2,
                vec![],
            ),
        ]
    }
}

/// Activity tone used by the runtime log.
/// 运行日志使用的活动级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    /// Informational note.
    /// 信息提示。
    Info,
    /// Successful milestone.
    /// 成功里程碑。
    Success,
    /// Attention-worthy warning.
    /// 需要关注的警告。
    Warning,
}

impl ActivityLevel {
    /// Returns a concise badge label for the UI.
    /// 返回界面使用的精简徽标文本。
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Success => "DONE",
            Self::Warning => "WARN",
        }
    }
}

/// Structured activity entry shown in the inspector log.
/// 展示在检查面板日志中的结构化活动项。
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    /// Activity severity or tone.
    /// 活动的严重程度或语气。
    pub level: ActivityLevel,
    /// Activity headline.
    /// 活动标题。
    pub title: String,
    /// Activity supporting details.
    /// 活动的补充说明。
    pub detail: String,
}

impl ActivityEntry {
    /// Creates a new activity entry from strongly typed parts.
    /// 使用强类型片段创建新的活动日志项。
    pub fn new(level: ActivityLevel, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            level,
            title: title.into(),
            detail: detail.into(),
        }
    }

    /// Builds seed runtime logs for the first prototype window.
    /// 为首版原型窗口构建种子运行日志。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                ActivityLevel::Success,
                "工作区骨架已准备",
                "已规划 Cargo workspace、GUI crate 与后续扩展模块边界。",
            ),
            Self::new(
                ActivityLevel::Info,
                "iced 组件接入中",
                "当前窗口将同时验证 pane_grid、text_editor、markdown 与 svg 等关键控件。",
            ),
            Self::new(
                ActivityLevel::Warning,
                "真实 AI 服务尚未接入",
                "当前版本聚焦桌面基础设施与交互骨架，网络编排与代理协议留待下一阶段。",
            ),
        ]
    }
}
