use crate::source::hashing::stable_text_hash;
use crate::types::ToolDescriptor;

/// Provider base prompt snapshot.
/// Provider 基础提示词快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPromptSnapshot {
    /// Prompt body text.
    /// 提示词正文。
    pub text: String,
    /// Stable text fingerprint.
    /// 稳定文本指纹。
    pub fingerprint: u64,
}

impl ProviderPromptSnapshot {
    /// Build one provider prompt snapshot from text.
    /// 从文本构建一个 Provider 提示词快照。
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let fingerprint = stable_text_hash(&text);
        Self { text, fingerprint }
    }
}

/// Agent base prompt snapshot.
/// Agent 基础提示词快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPromptSnapshot {
    /// Prompt body text.
    /// 提示词正文。
    pub text: String,
    /// Stable text fingerprint.
    /// 稳定文本指纹。
    pub fingerprint: u64,
}

impl AgentPromptSnapshot {
    /// Build one agent prompt snapshot from text.
    /// 从文本构建一个 Agent 提示词快照。
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let fingerprint = stable_text_hash(&text);
        Self { text, fingerprint }
    }
}

/// Project-rule snapshot such as AGENTS.md.
/// 项目规则快照，例如 AGENTS.md。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRulesSnapshot {
    /// Source path or label.
    /// 来源路径或标签。
    pub source: String,
    /// Full rule text.
    /// 完整规则文本。
    pub text: String,
    /// Stable content hash.
    /// 稳定内容哈希。
    pub content_hash: u64,
}

impl ProjectRulesSnapshot {
    /// Build one project-rules snapshot from source and text.
    /// 从来源与文本构建项目规则快照。
    pub fn new(source: impl Into<String>, text: impl Into<String>) -> Self {
        let source = source.into();
        let text = text.into();
        let content_hash = stable_text_hash(&text);
        Self {
            source,
            text,
            content_hash,
        }
    }
}

/// Environment context snapshot.
/// 环境上下文快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentContextSnapshot {
    /// Rendered environment body.
    /// 渲染后的环境正文。
    pub text: String,
    /// Stable text fingerprint.
    /// 稳定文本指纹。
    pub fingerprint: u64,
}

impl EnvironmentContextSnapshot {
    /// Build one environment snapshot from text.
    /// 从文本构建环境快照。
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let fingerprint = stable_text_hash(&text);
        Self { text, fingerprint }
    }
}

/// Task-goal snapshot.
/// 任务目标快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGoalSnapshot {
    /// Rendered goal body.
    /// 渲染后的目标正文。
    pub text: String,
    /// Stable text fingerprint.
    /// 稳定文本指纹。
    pub fingerprint: u64,
}

impl TaskGoalSnapshot {
    /// Build one task-goal snapshot from text.
    /// 从文本构建任务目标快照。
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let fingerprint = stable_text_hash(&text);
        Self { text, fingerprint }
    }
}

/// Visible tool catalog snapshot.
/// 可见工具目录快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCatalogSnapshot {
    /// Visible tool definitions.
    /// 可见工具定义。
    pub tools: Vec<ToolDescriptor>,
    /// Stable catalog fingerprint.
    /// 稳定目录指纹。
    pub fingerprint: u64,
}

impl ToolCatalogSnapshot {
    /// Build one tool-catalog snapshot from tool descriptors.
    /// 从工具描述构建一个工具目录快照。
    pub fn new(tools: Vec<ToolDescriptor>) -> Self {
        let stable_body = tools
            .iter()
            .map(|tool| {
                format!(
                    "{}|{}|{}|{}|{}",
                    tool.id,
                    tool.title,
                    tool.description,
                    tool.help_text.as_deref().unwrap_or_default(),
                    tool.schema_fingerprint.as_deref().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let fingerprint = stable_text_hash(&stable_body);
        Self { tools, fingerprint }
    }
}

/// Subagent mode snapshot.
/// 子代理模式快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentSnapshot {
    /// Stable subagent identity label.
    /// 稳定子代理身份标签。
    pub identity: String,
    /// Current delegated task text.
    /// 当前委托任务文本。
    pub task: String,
    /// Optional delegation guidance from the parent agent.
    /// 父代理附加的可选委托说明。
    pub delegation: Option<String>,
    /// Optional functional-mode system prompt supplement.
    /// 功能模式下可选的系统提示词补充。
    pub functional_system_prompt: Option<String>,
}

impl SubagentSnapshot {
    /// Compute a stable snapshot fingerprint.
    /// 计算稳定快照指纹。
    pub fn fingerprint(&self) -> u64 {
        stable_text_hash(&format!(
            "{}|{}|{}|{}",
            self.identity,
            self.task,
            self.delegation.as_deref().unwrap_or_default(),
            self.functional_system_prompt.as_deref().unwrap_or_default()
        ))
    }
}
