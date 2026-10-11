//! Structured activity-log models shared by the workbench and persistence projections.
//! 供工作台与持久化投影共享的结构化活动日志模型。

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
                "Workspace skeleton prepared",
                "The Cargo workspace, WebUI crate and extension module boundaries are planned.",
            ),
            Self::new(
                ActivityLevel::Info,
                "WebUI components being integrated",
                "This window verifies key controls such as pane_grid, text_editor, markdown and svg.",
            ),
            Self::new(
                ActivityLevel::Warning,
                "Real AI service not connected yet",
                "This version focuses on desktop infrastructure and the interaction skeleton; network orchestration and agent protocols come in the next stage.",
            ),
        ]
    }
}
