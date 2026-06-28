use super::*;

/// Describes one startup step state exposed by the Core Service.
/// 描述 Core Service 对外暴露的单个启动步骤状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupStepSnapshot {
    /// Stable startup step identifier used by clients to merge updates.
    /// 客户端用于合并更新的稳定启动步骤标识。
    pub id: String,
    /// User-facing startup step label.
    /// 面向用户显示的启动步骤名称。
    pub label: String,
    /// Current startup step status.
    /// 当前启动步骤状态。
    pub status: StartupStepStatus,
    /// Optional detailed startup step message.
    /// 可选的启动步骤详细消息。
    pub message: Option<String>,
    /// Optional timestamp for the most recent status update.
    /// 最近一次状态更新的可选时间戳。
    pub updated_at: Option<OffsetDateTime>,
}

/// Represents the lifecycle status of one startup step.
/// 表示单个启动步骤的生命周期状态。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum StartupStepStatus {
    /// The startup step has not started yet.
    /// 启动步骤尚未开始。
    Pending,
    /// The startup step is currently running.
    /// 启动步骤正在运行。
    Running,
    /// The startup step completed successfully.
    /// 启动步骤已成功完成。
    Ready,
    /// The startup step completed with a warning.
    /// 启动步骤已完成但存在警告。
    Warning,
    /// The startup step failed.
    /// 启动步骤执行失败。
    Error,
}

/// Provides a complete startup snapshot for the WebUI.
/// 为 WebUI 提供完整启动快照。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StartupSnapshot {
    /// Overall service readiness flag.
    /// 服务整体是否就绪。
    pub ready: bool,
    /// WebUI locale resolved from the persisted configuration when available.
    /// 从持久化配置解析出的 WebUI 语言，可用时用于启动前界面。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_locale: Option<String>,
    /// Current service startup phase label.
    /// 当前服务启动阶段标签。
    pub phase: String,
    /// Ordered startup steps displayed by clients.
    /// 客户端展示的有序启动步骤。
    pub steps: Vec<StartupStepSnapshot>,
    /// Recent startup log lines.
    /// 最近启动日志行。
    pub recent_logs: Vec<String>,
}

/// Summarizes one runtime module shown in the startup and status panels.
/// 汇总启动与状态面板中展示的单个运行时模块。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeModuleSnapshot {
    /// Stable module identifier.
    /// 稳定模块标识。
    pub id: String,
    /// User-facing module label.
    /// 面向用户显示的模块名称。
    pub label: String,
    /// Current module status.
    /// 当前模块状态。
    pub status: StartupStepStatus,
    /// Short module status message.
    /// 简短模块状态消息。
    pub message: String,
}
