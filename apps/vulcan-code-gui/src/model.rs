//! Domain models for the AI coding desktop scaffold.
//! AI 编程桌面骨架的领域模型定义。

use serde::{Deserialize, Serialize};
use std::fmt;

/// Available provider kinds supported by the workbench catalog.
/// 工作台目录中支持的供应商类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    /// OpenAI hosted models.
    /// OpenAI 托管模型。
    OpenAi,
    /// Anthropic hosted models.
    /// Anthropic 托管模型。
    Anthropic,
    /// Azure OpenAI hosted models.
    /// Azure OpenAI 托管模型。
    AzureOpenAi,
    /// ChatGPT first-party consumer surface.
    /// ChatGPT 官方消费端接口。
    ChatGpt,
    /// Cohere hosted models.
    /// Cohere 托管模型。
    Cohere,
    /// GitHub Copilot model surface.
    /// GitHub Copilot 模型接口。
    Copilot,
    /// DeepSeek hosted models.
    /// DeepSeek 托管模型。
    DeepSeek,
    /// Galadriel hosted models.
    /// Galadriel 托管模型。
    Galadriel,
    /// Google Gemini hosted models.
    /// Google Gemini 托管模型。
    Gemini,
    /// Groq hosted models.
    /// Groq 托管模型。
    Groq,
    /// Hugging Face inference models.
    /// Hugging Face 推理模型。
    HuggingFace,
    /// Hyperbolic hosted models.
    /// Hyperbolic 托管模型。
    Hyperbolic,
    /// Llamafile local-compatible models.
    /// Llamafile 本地兼容模型。
    Llamafile,
    /// MiniMax hosted models.
    /// MiniMax 托管模型。
    MiniMax,
    /// Mira hosted models.
    /// Mira 托管模型。
    Mira,
    /// Mistral hosted models.
    /// Mistral 托管模型。
    Mistral,
    /// Moonshot hosted models.
    /// Moonshot 托管模型。
    Moonshot,
    /// Ollama local models.
    /// Ollama 本地模型。
    Ollama,
    /// OpenRouter brokered models.
    /// OpenRouter 聚合模型。
    OpenRouter,
    /// Perplexity hosted models.
    /// Perplexity 托管模型。
    Perplexity,
    /// Together hosted models.
    /// Together 托管模型。
    Together,
    /// VoyageAI embedding-first models.
    /// VoyageAI 以向量为主的模型。
    VoyageAi,
    /// xAI hosted models.
    /// xAI 托管模型。
    Xai,
    /// Xiaomi Mimo hosted models.
    /// Xiaomi Mimo 托管模型。
    XiaomiMimo,
    /// Z.ai hosted models.
    /// Z.ai 托管模型。
    ZAi,
    /// Local or self-hosted OpenAI-compatible gateways.
    /// 本地或自托管 OpenAI 兼容网关。
    OpenAiCompatibleLocal,
}

impl Provider {
    /// All supported provider kinds exposed in the configuration selector.
    /// 配置选择器中暴露的全部供应商类型。
    pub const ALL: [Self; 26] = [
        Self::OpenAi,
        Self::Anthropic,
        Self::AzureOpenAi,
        Self::ChatGpt,
        Self::Cohere,
        Self::Copilot,
        Self::DeepSeek,
        Self::Galadriel,
        Self::Gemini,
        Self::Groq,
        Self::HuggingFace,
        Self::Hyperbolic,
        Self::Llamafile,
        Self::MiniMax,
        Self::Mira,
        Self::Mistral,
        Self::Moonshot,
        Self::Ollama,
        Self::OpenRouter,
        Self::Perplexity,
        Self::Together,
        Self::VoyageAi,
        Self::Xai,
        Self::XiaomiMimo,
        Self::ZAi,
        Self::OpenAiCompatibleLocal,
    ];
}

impl fmt::Display for Provider {
    /// Renders a user-facing provider label.
    /// 渲染面向用户的供应商标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::AzureOpenAi => "Azure OpenAI",
            Self::ChatGpt => "ChatGPT",
            Self::Cohere => "Cohere",
            Self::Copilot => "GitHub Copilot",
            Self::DeepSeek => "DeepSeek",
            Self::Galadriel => "Galadriel",
            Self::Gemini => "Gemini",
            Self::Groq => "Groq",
            Self::HuggingFace => "Hugging Face",
            Self::Hyperbolic => "Hyperbolic",
            Self::Llamafile => "Llamafile",
            Self::MiniMax => "MiniMax",
            Self::Mira => "Mira",
            Self::Mistral => "Mistral",
            Self::Moonshot => "Moonshot",
            Self::Ollama => "Ollama",
            Self::OpenRouter => "OpenRouter",
            Self::Perplexity => "Perplexity",
            Self::Together => "Together",
            Self::VoyageAi => "VoyageAI",
            Self::Xai => "xAI",
            Self::XiaomiMimo => "Xiaomi Mimo",
            Self::ZAi => "Z.ai",
            Self::OpenAiCompatibleLocal => "OpenAI-Compatible Local",
        };

        formatter.write_str(label)
    }
}

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
    ) -> Self {
        Self {
            id,
            title: title.into(),
            focus: focus.into(),
            status,
            changed_files,
            queued_steps,
        }
    }

    /// Builds seed session data for the first prototype window.
    /// 为首版原型窗口构建种子会话数据。
    pub fn seeded() -> Vec<Self> {
        vec![
            Self::new(
                1,
                "Rust + iced 基础设施搭建",
                "工作区、组件接入与界面骨架",
                SessionState::Running,
                7,
                4,
            ),
            Self::new(
                2,
                "后续能力规划",
                "命令执行、文件树与流式响应",
                SessionState::Completed,
                3,
                2,
            ),
            Self::new(
                3,
                "设计风险核对",
                "跨平台窗口与本地沙箱策略",
                SessionState::Blocked,
                2,
                1,
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
