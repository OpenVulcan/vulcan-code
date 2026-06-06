//! Session and project-facing identity models shared across the workbench.
//! 在工作台各层之间共享的会话与项目身份模型。

use std::fmt;
use uuid::Uuid;

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

/// Stable UUID session identifier used across the workbench, persistence, and runtime layers.
/// 跨工作台、持久化与运行时层统一使用的稳定 UUID 会话标识。
pub type SessionId = Uuid;

/// Builds one stable session record identifier from the canonical conversation record ordinal instead of the visible transcript window length.
/// 基于规范会话记录序号而不是可见转录窗口长度，构建一条稳定的会话记录标识。
pub fn build_session_record_message_id(
    session_id: SessionId,
    created_at: &str,
    conversation_record_ordinal: usize,
) -> String {
    format!("session-{session_id}-message-{created_at}-{conversation_record_ordinal}")
}

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
    /// Raw last-updated timestamp as Unix milliseconds text.
    /// 以 Unix 毫秒文本保存的原始最近更新时间。
    pub updated_at: String,
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
            updated_at: String::new(),
        }
    }

    /// Overrides the title source attached to the session summary.
    /// 覆盖附着在会话摘要上的标题来源。
    pub fn with_title_source(mut self, title_source: SessionTitleSource) -> Self {
        self.title_source = title_source;
        self
    }

    /// Overrides the raw last-updated timestamp used by dynamic clients.
    /// 覆盖动态客户端使用的原始最近更新时间。
    pub fn with_updated_at(mut self, updated_at: impl Into<String>) -> Self {
        self.updated_at = updated_at.into();
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
    /// Conversation hooks and memory recall are preparing context for the next request.
    /// 会话 hook 与记忆召回正在为下一次请求准备上下文。
    MemoryRecall,
    /// Provider request is in flight.
    /// Provider 请求正在进行。
    Requesting,
    /// Provider reasoning output is streaming.
    /// Provider 思考输出正在流式返回。
    Thinking,
    /// A provider-visible tool is currently running.
    /// Provider 可见工具当前正在运行。
    ToolRunning,
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

impl fmt::Display for SessionRunPhase {
    /// Renders one compact workbench-facing label for the current session phase.
    /// 为当前会话阶段渲染一条紧凑的工作台可见标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Idle => "Idle",
            Self::Preparing => "Preparing",
            Self::MemoryRecall => "MemoryRecall",
            Self::Requesting => "Requesting",
            Self::Thinking => "Thinking",
            Self::ToolRunning => "ToolRunning",
            Self::Ready => "Ready",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
        };
        formatter.write_str(label)
    }
}
