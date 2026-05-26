//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use crate::provider_config::{OpenRouterEndpointRouteSelection, ReasoningEffortLevel};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use uuid::Uuid;
use vulcan_context::SessionConversationState;
pub use vulcan_context::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};

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
                r"D:\projects\LuaSkills",
                false,
                0,
                vec![Uuid::from_u128(1), Uuid::from_u128(2)],
            ),
            Self::new(
                "vulcan-memory-mesh",
                "VulcanMemoryMesh",
                r"D:\projects\VulcanMemoryMesh",
                false,
                1,
                vec![Uuid::from_u128(3), Uuid::from_u128(4)],
            ),
            Self::new(
                "vulcan-plugins-opencode",
                "vulcan-plugins-opencode",
                r"D:\projects\vulcan-plugins-opencode",
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

/// Stable source kind attached to one pending session prompt queue entry.
/// 附着在单条待发送会话提示词队列项上的稳定来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingSessionPromptSource {
    /// User-authored follow-up that should wait for the current run to finish naturally.
    /// 应等待当前运行自然结束后再发送的用户跟进消息。
    UserFollowup,
    /// User-authored follow-up that should interrupt the current run before being dispatched.
    /// 应先打断当前运行再发送的用户跟进消息。
    UserInterrupt,
    /// Synthetic follow-up generated when one delegated subagent reports back asynchronously.
    /// 某个委派子代理异步回报后生成的合成跟进消息。
    DelegateCompletion,
}

/// Runtime controls captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的运行控制参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingPromptWebSearchOverride {
    /// Follow the globally configured web-search routing mode.
    /// 跟随全局配置的网页搜索路由模式。
    FollowGlobal,
    /// Force provider-native web search only.
    /// 强制仅使用模型原生网页搜索。
    NativeOnly,
    /// Force the configured third-party search chain only.
    /// 强制仅使用已配置的第三方搜索链。
    ThirdPartyOnly,
    /// Disable unified web search for this queued prompt.
    /// 为当前排队提示词关闭统一网页搜索。
    Disabled,
    /// Force one specific third-party provider entry.
    /// 强制使用单个指定的第三方搜索条目。
    SpecificProvider,
}

impl Default for PendingPromptWebSearchOverride {
    /// Returns the default queued web-search override behavior.
    /// 返回排队提示词默认使用的网页搜索覆盖行为。
    fn default() -> Self {
        Self::FollowGlobal
    }
}

/// Runtime controls captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的运行控制参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingPromptRuntimeOptions {
    /// Shared reasoning-effort preset that must travel with the queued prompt.
    /// 必须随排队提示词一起传递的共享推理档位预设。
    pub reasoning_effort: ReasoningEffortLevel,
    /// Optional sampling temperature override captured from the workbench controls.
    /// 从工作台控制区捕获的可选采样温度覆盖值。
    pub temperature: Option<f32>,
    /// Queued web-search override mode captured from the workbench controls.
    /// 从工作台控制区捕获的排队网页搜索覆盖模式。
    #[serde(default)]
    pub web_search_override: PendingPromptWebSearchOverride,
    /// Optional third-party provider id captured when one specific search entry is selected.
    /// 当选中单个指定搜索条目时捕获的可选第三方供应商标识。
    #[serde(default)]
    pub web_search_provider_id: Option<String>,
    /// Semantic AI capability kinds temporarily disabled for the current send chain without mutating persisted target preferences.
    /// 在不修改持久化目标偏好的前提下，为当前发送链路临时关闭的语义 AI 能力类型集合。
    #[serde(default)]
    pub disabled_ai_capability_kinds: BTreeSet<crate::provider_config::AiToolKind>,
}

impl Default for PendingPromptRuntimeOptions {
    /// Returns the neutral runtime option snapshot used when persisted queue entries predate explicit snapshots.
    /// 返回早于显式快照的持久化队列项所使用的中性运行参数快照。
    fn default() -> Self {
        Self {
            reasoning_effort: ReasoningEffortLevel::Medium,
            temperature: None,
            web_search_override: PendingPromptWebSearchOverride::default(),
            web_search_provider_id: None,
            disabled_ai_capability_kinds: BTreeSet::new(),
        }
    }
}

/// Provider/model identity captured when a queued prompt is authored or inserted.
/// 在排队提示词创建或插入时捕获的供应商与模型身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPromptModelReference {
    /// Stable provider identifier used to resolve credentials and transport configuration.
    /// 用于解析凭据与传输配置的稳定供应商标识。
    pub provider_id: String,
    /// Stable model identifier used by the provider request.
    /// Provider 请求使用的稳定模型标识。
    pub model_id: String,
    /// Human-readable provider label retained for queue diagnostics.
    /// 为队列诊断保留的人类可读供应商标签。
    #[serde(default)]
    pub provider_label: String,
    /// Human-readable model label retained for queue diagnostics.
    /// 为队列诊断保留的人类可读模型标签。
    #[serde(default)]
    pub model_name: String,
    /// Optional OpenRouter endpoint route locked together with the queued provider/model identity.
    /// 与排队供应商/模型身份一起锁定的可选 OpenRouter 端点路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_endpoint_route: Option<OpenRouterEndpointRouteSelection>,
}

/// One persisted pending session prompt retained for queued follow-ups and delegated-task callbacks.
/// 为排队跟进与委派任务回调保留的一条持久化待发送会话提示词。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSessionPromptEntry {
    /// Stable queue entry identifier used for deduplication and persistence.
    /// 用于去重与持久化的稳定队列项标识。
    pub id: String,
    /// Full prompt text that should be dispatched into the next session run.
    /// 应在下一次会话运行中发送的完整提示词文本。
    pub prompt: String,
    /// Compact preview text rendered inside the pending-queue strip.
    /// 渲染在待发送队列条中的紧凑预览文本。
    pub preview: String,
    /// Structured attachments that should be replayed together with the queued prompt text.
    /// 应与排队提示词文本一起回放的结构化附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Optional provider/model identity locked when the prompt entered a queue.
    /// 提示词进入队列时锁定的可选供应商与模型身份。
    #[serde(default)]
    pub model_reference: Option<PendingPromptModelReference>,
    /// Runtime controls locked when the prompt entered a queue.
    /// 提示词进入队列时锁定的运行控制参数。
    #[serde(default)]
    pub runtime_options: PendingPromptRuntimeOptions,
    /// Stable source kind attached to the queued prompt.
    /// 附着在该排队提示词上的稳定来源类型。
    pub source: PendingSessionPromptSource,
    /// Optional delegated task identifier associated with the queued follow-up.
    /// 与该排队跟进关联的可选委派任务标识。
    pub task_id: Option<String>,
    /// Optional visible-panel queue entry identifier from which this system prompt was inserted.
    /// 该系统提示词由哪个可见面板队列项插入而来的可选标识。
    #[serde(default)]
    pub panel_entry_id: Option<String>,
    /// Unix-millis text captured when the queue entry was created.
    /// 捕获该队列项创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Stable state retained by one visible prompt-panel queue entry shown above the composer.
/// 显示在提示词输入框上方的可见队列项所保留的稳定状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingPromptPanelEntryState {
    /// Entry is still waiting for an explicit user insertion into the internal system queue.
    /// 当前条目仍在等待用户显式插入到内部系统队列。
    Ready,
    /// Entry has already been inserted into the internal system queue and is now waiting there silently.
    /// 当前条目已经插入到内部系统队列，并正在该队列中静默等待。
    Inserted,
}

impl Default for PendingPromptPanelEntryState {
    /// Returns the default visible prompt-panel entry state used for older persisted snapshots.
    /// 返回旧持久化快照恢复时使用的默认可见队列项状态。
    fn default() -> Self {
        Self::Ready
    }
}

/// One visible user-authored prompt retained inside the dedicated insertion panel above the composer.
/// 保留在提示词输入框上方独立插入面板中的一条用户可见提示词项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPromptPanelEntry {
    /// Stable panel-entry identifier used for reordering, editing, and insertion tracking.
    /// 用于重排、编辑与插入跟踪的稳定面板项标识。
    pub id: String,
    /// Full user-authored prompt text retained until it is explicitly inserted or removed.
    /// 在被显式插入或移除之前一直保留的完整用户提示词文本。
    pub prompt: String,
    /// Compact preview text rendered inside the visible prompt-panel row.
    /// 渲染在可见提示词面板行中的紧凑预览文本。
    pub preview: String,
    /// Structured attachments retained until the row is inserted or removed.
    /// 在该行被插入或移除前一直保留的结构化附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Optional provider/model identity locked when the row entered the visible queue.
    /// 可见队列项进入队列时锁定的可选供应商与模型身份。
    #[serde(default)]
    pub model_reference: Option<PendingPromptModelReference>,
    /// Runtime controls locked when the row entered the visible queue.
    /// 可见队列项进入队列时锁定的运行控制参数。
    #[serde(default)]
    pub runtime_options: PendingPromptRuntimeOptions,
    /// Stable insertion state used to hide controls after the prompt has entered the internal system queue.
    /// 提示词进入内部系统队列后用于隐藏控制按钮的稳定插入状态。
    #[serde(default)]
    pub state: PendingPromptPanelEntryState,
    /// Unix-millis text captured when the visible prompt-panel entry was created.
    /// 捕获该可见提示词面板项创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionComposerDraftState {
    /// Draft text currently staged inside the session-scoped composer.
    /// 当前暂存在按会话归属输入框中的草稿文本。
    #[serde(default)]
    pub text: String,
    /// Draft attachments currently staged inside the session-scoped composer.
    /// 当前暂存在按会话归属输入框中的草稿附件集合。
    #[serde(default)]
    pub attachments: Vec<ConversationAttachment>,
    /// Vertical scroll offset mirrored from the session-scoped composer viewport.
    /// 从按会话归属输入框视口镜像出的垂直滚动偏移。
    #[serde(default)]
    pub scroll_offset: f32,
}

impl SessionComposerDraftState {
    /// Returns whether the draft carries no staged text, attachments, or scroll state.
    /// 返回该草稿是否不包含任何已暂存文本、附件或滚动状态。
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.attachments.is_empty() && self.scroll_offset <= 0.0
    }
}

/// Terminal status recorded for one completed or aborted turn statistics snapshot.
/// 为单次已完成或已中止轮次统计快照记录的终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnStatsStatus {
    /// Turn completed successfully and produced a summary.
    /// 该轮次成功完成并产出了摘要。
    Completed,
    /// Turn terminated because the run failed.
    /// 该轮次因执行失败而终止。
    Failed,
    /// Turn terminated because the run was stopped manually or interrupted.
    /// 该轮次因手动停止或主动中断而终止。
    Stopped,
}

impl SessionTurnStatsStatus {
    /// Returns the compact persisted label exposed to transcript metadata and chips.
    /// 返回暴露给转录元数据与统计标签的紧凑持久化文本。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    /// Returns the human-readable label shown in transcript summaries and hover details.
    /// 返回展示在转录摘要与悬浮详情中的人类可读标签。
    pub const fn display_label(self) -> &'static str {
        match self {
            Self::Completed => "已完成",
            Self::Failed => "失败",
            Self::Stopped => "已中止",
        }
    }
}

/// Aggregated per-turn statistics derived after one whole turn reaches a terminal state.
/// 在单个完整轮次到达终态后派生出的按轮次聚合统计快照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionTurnStats {
    /// Stable status describing how the turn ended.
    /// 描述该轮次如何结束的稳定状态。
    pub status: SessionTurnStatsStatus,
    /// Number of provider calls aggregated inside the whole turn.
    /// 在整个轮次内聚合得到的 Provider 调用次数。
    #[serde(default)]
    pub provider_call_count: usize,
    /// Number of tool-call records aggregated inside the whole turn.
    /// 在整个轮次内聚合得到的工具调用记录数量。
    #[serde(default)]
    pub tool_call_count: usize,
    /// Provider-reported total input tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告输入 Token 总数。
    #[serde(default)]
    pub input_tokens: usize,
    /// Provider-reported total output tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告输出 Token 总数。
    #[serde(default)]
    pub output_tokens: usize,
    /// Provider-reported total reasoning tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告推理 Token 总数。
    #[serde(default)]
    pub reasoning_tokens: usize,
    /// Provider-reported total tokens aggregated across the turn.
    /// 跨整个轮次聚合得到的 Provider 报告总 Token 数。
    #[serde(default)]
    pub total_tokens: usize,
    /// Optional provider cache-hit token count when one official field is available.
    /// 当官方字段可用时记录的可选 Provider 缓存命中 Token 数。
    #[serde(default)]
    pub cached_input_tokens: Option<usize>,
    /// Optional provider cache-write token count when one official field is available.
    /// 当官方字段可用时记录的可选 Provider 缓存写入 Token 数。
    #[serde(default)]
    pub cache_creation_input_tokens: Option<usize>,
    /// Best-effort duration of the whole turn in milliseconds.
    /// 整个轮次的最佳努力持续时长，单位毫秒。
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Best-effort output-token speed derived from official output tokens and turn duration.
    /// 根据官方输出 Token 与轮次耗时推导出的最佳努力输出速度。
    #[serde(default)]
    pub output_tokens_per_second: Option<f64>,
}

impl Default for SessionTurnStatsStatus {
    /// Returns the default turn status used by legacy deserialization fallbacks.
    /// 返回旧版本反序列化回退时使用的默认轮次状态。
    fn default() -> Self {
        Self::Completed
    }
}

impl SessionTurnStats {
    /// Returns a compact one-line summary used by transcript chips and lightweight projections.
    /// 返回供转录统计标签与轻量投影使用的紧凑单行摘要。
    pub fn summary_line(&self) -> String {
        let mut parts = vec![
            format!("{} 次调用", self.provider_call_count),
            format!("{} 个工具", self.tool_call_count),
        ];
        if self.total_tokens > 0 {
            parts.push(format!("{} tokens", self.total_tokens));
        }
        if let Some(cached_input_tokens) = self.cached_input_tokens
            && cached_input_tokens > 0
        {
            parts.push(format!("缓存命中 {}", cached_input_tokens));
        }
        if let Some(speed) = self.output_tokens_per_second
            && speed.is_finite()
            && speed > 0.0
        {
            parts.push(format!("{speed:.1}/s"));
        }
        parts.join(" · ")
    }

    /// Returns a multi-line human-readable detail body used by raw-source and future hover/detail surfaces.
    /// 返回供原文与后续悬浮或详情表面使用的多行人类可读详情正文。
    pub fn detail_body(&self) -> String {
        let mut lines = vec![
            format!("状态：{}", self.status.display_label()),
            format!("Provider 调用：{}", self.provider_call_count),
            format!("工具调用：{}", self.tool_call_count),
            format!("输入 Tokens：{}", self.input_tokens),
            format!("输出 Tokens：{}", self.output_tokens),
            format!("思考 Tokens：{}", self.reasoning_tokens),
            format!("总 Tokens：{}", self.total_tokens),
        ];
        if let Some(duration_ms) = self.duration_ms {
            lines.push(format!("耗时：{duration_ms} ms"));
        }
        if let Some(speed) = self.output_tokens_per_second
            && speed.is_finite()
        {
            lines.push(format!("输出速度：{speed:.3} tokens/s"));
        }
        if let Some(cached_input_tokens) = self.cached_input_tokens {
            lines.push(format!("缓存命中 Tokens：{cached_input_tokens}"));
        }
        if let Some(cache_creation_input_tokens) = self.cache_creation_input_tokens {
            lines.push(format!("缓存写入 Tokens：{cache_creation_input_tokens}"));
        }
        lines.join("\n")
    }
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTranscriptUiState {
    /// Collapsed visible message identifiers persisted across restarts.
    /// 跨重启持久化保存的可见消息折叠标识集合。
    #[serde(default)]
    pub collapsed_message_ids: BTreeSet<String>,
    /// Pending prompt queue persisted with the session so queued follow-ups survive reloads.
    /// 随会话一起持久化保存的待发送提示词队列，用于让排队跟进跨重载保留。
    #[serde(default)]
    pub pending_prompt_queue: Vec<PendingSessionPromptEntry>,
    /// Visible prompt-panel queue persisted with the session so user-authored staged inserts survive reloads independently from the internal system queue.
    /// 随会话一起持久化保存的可见提示词面板队列，使用户分批插入项可以独立于内部系统队列跨重载保留。
    #[serde(default)]
    pub pending_prompt_panel_queue: Vec<PendingPromptPanelEntry>,
    /// Composer draft state persisted per session so switching sessions restores independent input text and attachments.
    /// 按会话持久化保存的输入框草稿状态，用于在切换会话时恢复独立的输入文本与附件。
    #[serde(default)]
    pub composer_draft: SessionComposerDraftState,
}

impl SessionTranscriptUiState {
    /// Returns whether the transcript UI state carries no persisted user-visible state.
    /// 返回该转录界面状态是否不包含任何需要持久化的用户可见状态。
    pub fn is_empty(&self) -> bool {
        self.collapsed_message_ids.is_empty()
            && self.pending_prompt_queue.is_empty()
            && self.pending_prompt_panel_queue.is_empty()
            && self.composer_draft.is_empty()
    }
}

/// Session-scoped runtime state required for context reconstruction and agent tracing.
/// 上下文重建与代理追踪所需的按会话归属运行时状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Last active OpenRouter endpoint route locked by the session.
    /// 最近一次被该会话锁定使用的 OpenRouter 端点路由。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_openrouter_endpoint_route: Option<OpenRouterEndpointRouteSelection>,
    /// Latest compaction identifier attached to the session.
    /// 附着在该会话上的最近一次压缩标识。
    pub last_compaction_id: Option<String>,
    /// Current message identifier used as the next context reconstruction anchor.
    /// 当前用作下一轮上下文重建锚点的消息标识。
    pub context_start_message_id: Option<String>,
    /// Live budget fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时预算指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_budget_fingerprint: Option<String>,
    /// Live layered-context fingerprint captured from the latest provider-facing request shape.
    /// 从最近一次面向 Provider 请求形态捕获的实时分层上下文指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_context_assembly_fingerprint: Option<String>,
    /// Anchor budget fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点预算指纹。
    #[serde(
        default,
        alias = "last_context_budget_fingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub anchor_context_budget_fingerprint: Option<String>,
    /// Anchor layered-context fingerprint captured when the active compaction baseline was accepted.
    /// 在接受当前压缩基线时捕获的锚点分层上下文指纹。
    #[serde(
        default,
        alias = "last_context_assembly_fingerprint",
        skip_serializing_if = "Option::is_none"
    )]
    pub anchor_context_assembly_fingerprint: Option<String>,
    /// Last persisted provider-request preview Markdown used by the dedicated API request transcript tab.
    /// 供专用 API 请求转录页签使用的最近一次持久化 Provider 请求预览 Markdown。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_provider_request_preview_markdown: Option<String>,
    /// Persisted transcript UI state restored with the session runtime metadata.
    /// 随会话运行时元数据一起恢复的持久化转录界面状态。
    pub ui_state: SessionTranscriptUiState,
    /// Canonical conversation state used by the runtime instead of rebuilding history from transcript projections.
    /// 运行时使用的规范会话状态，用于替代从转录投影重建历史。
    pub conversation_state: SessionConversationState,
    /// Latest provider usage snapshot retained for budget calibration across turns.
    /// 为跨轮预算校准保留的最近一次 Provider 用量快照。
    pub last_provider_usage: Option<ProviderUsageSnapshot>,
    /// Latest successful provider usage snapshot captured from the final real provider turn.
    /// 从最终真实 Provider 回合中捕获到的最近一次成功用量快照。
    pub last_successful_provider_usage: Option<ProviderUsageSnapshot>,
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
