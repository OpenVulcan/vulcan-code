//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use crate::conversation_protocol::SessionConversationState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use uuid::Uuid;

/// Execution modes for AI coding tasks.
/// AI 编码任务的执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Executes implementation tasks directly with the full coding tool surface.
    /// 使用完整编码工具面直接执行实现任务。
    Build,
    /// Generates one plan before implementation with a read-only and analysis-biased tool surface.
    /// 在实现前生成方案，并使用只读与分析偏向工具面。
    Plan,
}

impl ExecutionMode {
    /// All execution modes exposed in the radio group.
    /// 单选组件中暴露的全部执行模式。
    pub const ALL: [Self; 2] = [Self::Build, Self::Plan];
}

impl fmt::Display for ExecutionMode {
    /// Renders a user-facing execution mode label.
    /// 渲染面向用户的执行模式标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Build => "Build",
            Self::Plan => "Plan",
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

/// Stable UUID session identifier used across the workbench, persistence, and runtime layers.
/// 跨工作台、持久化与运行时层统一使用的稳定 UUID 会话标识。
pub type SessionId = Uuid;

/// Lightweight summary of a coding session.
/// 编码会话的轻量级摘要信息。
#[derive(Debug, Clone)]
pub struct SessionSummary {
    /// Stable session identifier for selection.
    /// 用于选择状态的稳定会话标识。
    pub id: SessionId,
    /// Owning project identifier used to restore the session back into the correct project tree.
    /// 用于将会话恢复回正确项目树的所属项目标识。
    pub project_id: Option<String>,
    /// Session display title.
    /// 会话展示标题。
    pub title: String,
    /// Session title origin used to avoid overwriting manual titles blindly.
    /// 用于避免盲目覆盖人工标题的会话标题来源。
    pub title_source: SessionTitleSource,
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
        id: impl Into<SessionId>,
        project_id: Option<String>,
        title: impl Into<String>,
        focus: impl Into<String>,
        status: SessionState,
        changed_files: u8,
        queued_steps: u8,
        last_updated_label: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            project_id,
            title: title.into(),
            title_source: SessionTitleSource::Generated,
            focus: focus.into(),
            status,
            changed_files,
            queued_steps,
            last_updated_label: last_updated_label.into(),
        }
    }

    /// Overrides the title source attached to the session summary.
    /// 覆盖附着在会话摘要上的标题来源。
    pub fn with_title_source(mut self, title_source: SessionTitleSource) -> Self {
        self.title_source = title_source;
        self
    }

    /// Builds seed session data for the first prototype window.
    /// 为首版原型窗口构建种子会话数据。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                Uuid::from_u128(1),
                Some(String::from("vulcan-luaskills")),
                "重命名中枢系统",
                "收敛工作台命名与主入口语义",
                SessionState::Running,
                7,
                4,
                "1天前",
            )
            .with_title_source(SessionTitleSource::Seeded),
            Self::new(
                Uuid::from_u128(2),
                Some(String::from("vulcan-luaskills")),
                "评估 LuaSkills 会话声明机制",
                "梳理技能、工具与角色接入策略",
                SessionState::Completed,
                3,
                2,
                "3天前",
            )
            .with_title_source(SessionTitleSource::Seeded),
            Self::new(
                Uuid::from_u128(3),
                Some(String::from("vulcan-memory-mesh")),
                "检查构建脚本异常相关问题",
                "核对桌面窗口、打包与启动细节",
                SessionState::Blocked,
                2,
                1,
                "3周前",
            )
            .with_title_source(SessionTitleSource::Seeded),
            Self::new(
                Uuid::from_u128(4),
                Some(String::from("vulcan-memory-mesh")),
                "为 opencode.json 引用安装",
                "梳理供应商配置读取与安装体验",
                SessionState::Draft,
                1,
                2,
                "26天前",
            )
            .with_title_source(SessionTitleSource::Seeded),
        ]
    }
}

/// Title origin metadata stored alongside one session summary.
/// 与单个会话摘要一同存储的标题来源元数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTitleSource {
    /// Seed title bundled with prototype data.
    /// 随原型种子数据附带的初始标题。
    Seeded,
    /// Automatically generated title.
    /// 自动生成的标题。
    Generated,
    /// Manually edited title.
    /// 手工编辑后的标题。
    Manual,
}

impl SessionTitleSource {
    /// Returns the persisted text key used by the workspace store.
    /// 返回工作台存储层使用的持久化文本键。
    pub const fn as_persisted_key(self) -> &'static str {
        match self {
            Self::Seeded => "seeded",
            Self::Generated => "generated",
            Self::Manual => "manual",
        }
    }

    /// Parses one persisted text key back into the session-title source enum.
    /// 将一条持久化文本键解析回会话标题来源枚举。
    pub fn from_persisted_key(value: &str) -> Self {
        match value {
            "seeded" => Self::Seeded,
            "manual" => Self::Manual,
            _ => Self::Generated,
        }
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
        session_ids: Vec<SessionId>,
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
                vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            ),
            Self::new(
                "vulcan-memory-mesh",
                "VulcanMemoryMesh",
                "D:/projects/VulcanMemoryMesh",
                false,
                1,
                vec![Uuid::from_u128(3), Uuid::from_u128(4)],
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

/// Message kinds rendered inside one workbench session transcript.
/// 渲染在某个工作台会话转录区中的消息类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessageKind {
    /// User-authored prompt or follow-up input.
    /// 用户撰写的提示词或跟进输入。
    User,
    /// Assistant-authored execution or reasoning output.
    /// Assistant 生成的执行结果或推理输出。
    Assistant,
    /// Error output emitted by the execution pipeline.
    /// 执行链路输出的错误消息。
    Error,
}

impl SessionMessageKind {
    /// Returns the compact label displayed in one transcript bubble header.
    /// 返回显示在转录气泡头部中的精简标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Assistant => "AI",
            Self::Error => "ERROR",
        }
    }
}

/// Durable record kinds used for visible transcript rows and hidden internal nodes.
/// 用于可见转录行与隐藏内部节点的持久化记录类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessageRecordKind {
    /// Plain transcript row shown directly to the user.
    /// 直接展示给用户的普通转录行。
    Transcript,
    /// Visible assistant thinking output kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的思考输出。
    ThinkingOutput,
    /// Visible assistant final output kept inside future model context.
    /// 展示给用户且纳入后续模型上下文的正式输出。
    AssistantOutput,
    /// Visible tool-call summary row kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的工具调用摘要行。
    ToolCallOutput,
    /// Visible tool-result row kept outside future model context.
    /// 展示给用户但不纳入后续模型上下文的工具返回行。
    ToolResultOutput,
    /// Hidden title-generation output kept for traceability.
    /// 为了可追溯性保留的隐藏标题生成输出。
    TitleOutput,
    /// Hidden summary output produced after one execution round.
    /// 在一次执行回合后产生的隐藏摘要输出。
    SummaryOutput,
    /// Hidden explore output produced when a session is seeded from one project.
    /// 从某个项目播种会话时产生的隐藏探索输出。
    ExploreOutput,
    /// Hidden compaction summary used as the new context anchor.
    /// 作为新上下文锚点使用的隐藏压缩摘要。
    CompactionSummary,
}

impl SessionMessageRecordKind {
    /// Returns the persisted text key used by the SQLite workspace store.
    /// 返回 SQLite 工作台存储使用的持久化文本键。
    pub const fn as_persisted_key(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::ThinkingOutput => "thinking-output",
            Self::AssistantOutput => "assistant-output",
            Self::ToolCallOutput => "tool-call-output",
            Self::ToolResultOutput => "tool-result-output",
            Self::TitleOutput => "title-output",
            Self::SummaryOutput => "summary-output",
            Self::ExploreOutput => "explore-output",
            Self::CompactionSummary => "compaction-summary",
        }
    }

    /// Returns whether the record should be included when rebuilding the next LLM context.
    /// 返回在重建下一轮 LLM 上下文时是否应纳入该记录。
    pub const fn contributes_to_context(self) -> bool {
        !matches!(
            self,
            Self::TitleOutput
                | Self::ThinkingOutput
                | Self::ToolCallOutput
                | Self::ToolResultOutput
        )
    }

    /// Parses one persisted text key back into the durable record kind enum.
    /// 将一条持久化文本键解析回持久化记录类型枚举。
    pub fn from_persisted_key(value: &str) -> Self {
        match value {
            "thinking-output" => Self::ThinkingOutput,
            "assistant-output" => Self::AssistantOutput,
            "tool-call-output" => Self::ToolCallOutput,
            "tool-result-output" => Self::ToolResultOutput,
            "title-output" => Self::TitleOutput,
            "summary-output" => Self::SummaryOutput,
            "explore-output" => Self::ExploreOutput,
            "compaction-summary" => Self::CompactionSummary,
            _ => Self::Transcript,
        }
    }
}

/// Structured message entry shown inside one workbench session transcript.
/// 展示在某个工作台会话转录区中的结构化消息项。
#[derive(Debug, Clone)]
pub struct SessionMessageEntry {
    /// Stable message identifier stored in SQLite and referenced by compaction checkpoints.
    /// 存储在 SQLite 中并被压缩检查点引用的稳定消息标识。
    pub id: String,
    /// High-level message kind.
    /// 消息的高层类型。
    pub kind: SessionMessageKind,
    /// Durable record kind distinguishing visible transcript rows from hidden internal outputs.
    /// 用于区分可见转录行与隐藏内部输出的持久化记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Whether the message should be rendered in the visible transcript stream.
    /// 该消息是否应渲染到可见转录流中。
    pub visible: bool,
    /// Optional headline displayed above the message body.
    /// 展示在消息正文上方的可选标题。
    pub title: String,
    /// Main message body content.
    /// 主要消息正文内容。
    pub body: String,
    /// Optional structured metadata reserved for transcript-only UI enrichments and audit details.
    /// 为仅限转录的界面增强与审计细节保留的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis text captured when the entry is created or hydrated from storage.
    /// 在创建或从存储恢复时捕获的 Unix 毫秒文本。
    pub created_at: String,
}

impl SessionMessageEntry {
    /// Creates a new session message entry from explicit parts.
    /// 使用明确片段创建新的会话消息项。
    pub fn new(
        kind: SessionMessageKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            kind,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: title.into(),
            body: body.into(),
            metadata: None,
            created_at: String::new(),
        }
    }

    /// Creates one hidden internal message entry with an explicit record kind.
    /// 使用显式记录类型创建一条隐藏内部消息项。
    pub fn hidden(
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: String::new(),
            kind,
            record_kind,
            visible: false,
            title: title.into(),
            body: body.into(),
            metadata: None,
            created_at: String::new(),
        }
    }

    /// Attaches persisted runtime metadata to a freshly created session message entry.
    /// 为一条新创建的会话消息项附着持久化运行时元数据。
    pub fn with_runtime_metadata(
        mut self,
        id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        self.id = id.into();
        self.created_at = created_at.into();
        self
    }

    /// Returns whether the entry should be shown to the user inside the transcript stream.
    /// 返回该消息项是否应展示给用户的转录流中。
    pub const fn is_visible_to_user(&self) -> bool {
        self.visible
    }

    /// Returns whether the entry should participate in future model-context reconstruction.
    /// 返回该消息项是否应参与未来的模型上下文重建。
    pub const fn contributes_to_context(&self) -> bool {
        self.record_kind.contributes_to_context()
    }
}

/// Hidden compaction checkpoint bound to one session transcript.
/// 绑定到单个会话转录流的隐藏压缩检查点。
#[derive(Debug, Clone)]
pub struct SessionCompactionEntry {
    /// Stable compaction identifier.
    /// 稳定的压缩标识。
    pub id: String,
    /// Last source message included by the compaction input window.
    /// 被本次压缩输入窗口覆盖的最后一条源消息标识。
    pub source_until_message_id: Option<String>,
    /// First message identifier that should be used when rebuilding the next context window.
    /// 重建下一轮上下文窗口时应作为起点的第一条消息标识。
    pub context_start_message_id: Option<String>,
    /// Hidden summary message emitted by the compaction step.
    /// 压缩步骤输出的隐藏摘要消息标识。
    pub summary_message_id: Option<String>,
    /// Unix-millis text capturing when the checkpoint was created.
    /// 捕获该检查点创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTranscriptUiState {
    /// Collapsed visible message identifiers persisted across restarts.
    /// 跨重启持久化保存的可见消息折叠标识集合。
    pub collapsed_message_ids: BTreeSet<String>,
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default)]
pub struct SessionRuntimeStateEntry {
    /// Last active agent identifier used by the session.
    /// 最近一次被该会话使用的激活代理标识。
    pub active_agent_id: Option<String>,
    /// Last active provider identifier used by the session.
    /// 最近一次被该会话使用的激活供应商标识。
    pub active_provider_id: Option<String>,
    /// Last active model identifier used by the session.
    /// 最近一次被该会话使用的激活模型标识。
    pub active_model_id: Option<String>,
    /// Latest compaction identifier attached to the session.
    /// 附着在该会话上的最近一次压缩标识。
    pub last_compaction_id: Option<String>,
    /// Current message identifier used as the next context reconstruction anchor.
    /// 当前用作下一轮上下文重建锚点的消息标识。
    pub context_start_message_id: Option<String>,
    /// Persisted transcript UI state restored with the session runtime metadata.
    /// 随会话运行时元数据一起恢复的持久化转录界面状态。
    pub ui_state: SessionTranscriptUiState,
    /// Canonical conversation state used by the runtime instead of rebuilding history from transcript projections.
    /// 运行时使用的规范会话状态，用于替代从转录投影重建历史。
    pub conversation_state: SessionConversationState,
    /// Unix-millis text capturing when the runtime state was last refreshed.
    /// 捕获运行时状态最近刷新时间的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Execution phases used by the main workbench transcript header.
/// 主工作台转录区头部使用的执行阶段类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRunPhase {
    /// No active execution has started yet.
    /// 尚未开始任何执行。
    Idle,
    /// Execution preparation or streaming is in progress.
    /// 执行准备或流式输出进行中。
    Preparing,
    /// The latest execution finished and produced output.
    /// 最近一次执行已完成并产生输出。
    Ready,
    /// The latest execution failed.
    /// 最近一次执行失败。
    Failed,
    /// Execution was stopped manually.
    /// 执行被手动停止。
    Stopped,
}

