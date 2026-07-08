use crate::policy::ExecutionRuntimePolicyRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Capability policy mode used by tool and prompt-skill selectors.
/// 工具与提示词技能选择器使用的能力策略模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityPolicyMode {
    /// Expose every capability allowed by the lower runtime layers.
    /// 暴露下层运行时允许的全部能力。
    #[default]
    All,
    /// Expose only selectors listed by the policy.
    /// 仅暴露策略列表中声明的选择器。
    AllowOnly,
    /// Expose every lower-layer capability except selectors listed by the policy.
    /// 暴露除策略列表声明项以外的下层能力。
    DenyOnly,
}

/// Selector policy snapshot shared by tools and prompt skills.
/// 工具与提示词技能共享的选择器策略快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPolicySnapshot {
    /// Policy mode that gives the selector list its meaning.
    /// 赋予选择器列表语义的策略模式。
    #[serde(default)]
    pub mode: CapabilityPolicyMode,
    /// Selectors used by allow-only or deny-only modes.
    /// 仅允许或仅禁止模式使用的选择器列表。
    #[serde(default)]
    pub selectors: Vec<String>,
}

/// Parses a stable capability policy mode key.
/// 解析稳定的能力策略模式键。
pub fn parse_capability_policy_mode_key(mode: &str) -> Option<CapabilityPolicyMode> {
    match mode.trim() {
        "all" => Some(CapabilityPolicyMode::All),
        "allow_only" | "allowOnly" => Some(CapabilityPolicyMode::AllowOnly),
        "deny_only" | "denyOnly" => Some(CapabilityPolicyMode::DenyOnly),
        _ => None,
    }
}

/// Normalizes one capability policy snapshot.
/// 规范化单个能力策略快照。
pub fn normalize_capability_policy_snapshot(
    mut policy: CapabilityPolicySnapshot,
) -> CapabilityPolicySnapshot {
    policy.selectors = normalized_capability_selector_list(std::mem::take(&mut policy.selectors));
    if policy.mode == CapabilityPolicyMode::All {
        policy.selectors.clear();
    }
    policy
}

/// Builds a capability policy from a mode key, selectors, or fallback allow/deny lists.
/// 通过模式键、选择器或回退允许/禁止列表构建能力策略。
pub fn capability_policy_from_mode_or_selector_lists(
    mode: Option<&str>,
    selectors: Vec<String>,
    fallback_allowed: Vec<String>,
    fallback_disabled: Vec<String>,
) -> CapabilityPolicySnapshot {
    if let Some(mode) = mode.and_then(parse_capability_policy_mode_key) {
        return normalize_capability_policy_snapshot(CapabilityPolicySnapshot { mode, selectors });
    }
    capability_policy_from_selector_lists(fallback_allowed, fallback_disabled)
}

/// Builds a capability policy from allow and deny selector lists.
/// 通过允许与禁止选择器列表构建能力策略。
pub fn capability_policy_from_selector_lists(
    allowed: Vec<String>,
    disabled: Vec<String>,
) -> CapabilityPolicySnapshot {
    let allowed = normalized_capability_selector_list(allowed);
    let disabled = normalized_capability_selector_list(disabled);
    if !allowed.is_empty() {
        let disabled_set = capability_selector_key_set(&disabled);
        return CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::AllowOnly,
            selectors: allowed
                .into_iter()
                .filter(|selector| !disabled_set.contains(&capability_selector_key(selector)))
                .collect(),
        };
    }
    if !disabled.is_empty() {
        return CapabilityPolicySnapshot {
            mode: CapabilityPolicyMode::DenyOnly,
            selectors: disabled,
        };
    }
    CapabilityPolicySnapshot::default()
}

/// Applies a capability policy to a tool exposure snapshot and its mirror lists.
/// 将能力策略应用到工具可见性快照及其镜像列表。
pub fn apply_tool_capability_policy(
    snapshot: &mut ToolExposureSnapshot,
    policy: CapabilityPolicySnapshot,
) {
    snapshot.policy = normalize_capability_policy_snapshot(policy);
    match snapshot.policy.mode {
        CapabilityPolicyMode::All => {
            snapshot.allowed_tools.clear();
            snapshot.disabled_tools.clear();
        }
        CapabilityPolicyMode::AllowOnly => {
            snapshot.allowed_tools = snapshot.policy.selectors.clone();
            snapshot.disabled_tools.clear();
        }
        CapabilityPolicyMode::DenyOnly => {
            snapshot.allowed_tools.clear();
            snapshot.disabled_tools = snapshot.policy.selectors.clone();
        }
    }
}

/// Applies a capability policy to a prompt-skill snapshot and its mirror lists.
/// 将能力策略应用到提示词技能快照及其镜像列表。
pub fn apply_prompt_skill_capability_policy(
    snapshot: &mut PromptSkillSnapshot,
    policy: CapabilityPolicySnapshot,
) {
    snapshot.policy = normalize_capability_policy_snapshot(policy);
    match snapshot.policy.mode {
        CapabilityPolicyMode::All => {
            snapshot.enabled.clear();
            snapshot.disabled.clear();
        }
        CapabilityPolicyMode::AllowOnly => {
            snapshot.enabled = snapshot.policy.selectors.clone();
            snapshot.disabled.clear();
        }
        CapabilityPolicyMode::DenyOnly => {
            snapshot.enabled.clear();
            snapshot.disabled = snapshot.policy.selectors.clone();
        }
    }
}

/// Normalizes selector text while preserving first occurrence order.
/// 规范化选择器文本并保留首次出现顺序。
fn normalized_capability_selector_list(values: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

/// Builds a normalized selector lookup set.
/// 构建规范化的选择器查找集合。
fn capability_selector_key_set(values: &[String]) -> std::collections::BTreeSet<String> {
    values
        .iter()
        .map(|value| capability_selector_key(value))
        .filter(|value| !value.is_empty())
        .collect()
}

/// Builds a normalized selector comparison key.
/// 构建规范化的选择器比较键。
fn capability_selector_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

/// Provider and model snapshot selected for one session node.
/// 单个会话节点选定的供应商与模型快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderModelSnapshot {
    /// Provider identifier.
    /// 供应商标识符。
    pub provider_id: Option<String>,
    /// Model identifier.
    /// 模型标识符。
    pub model_id: Option<String>,
    /// Reasoning effort or reasoning profile.
    /// 推理等级或推理配置。
    pub reasoning_effort: Option<String>,
    /// Execution mode selected for the node.
    /// 该节点选定的执行模式。
    pub execution_mode: Option<String>,
}

/// Tool visibility snapshot used by prompt assembly and execution.
/// 提示词组装与执行使用的工具可见性快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolExposureSnapshot {
    /// Unified tool selector policy.
    /// 统一工具选择器策略。
    #[serde(default)]
    pub policy: CapabilityPolicySnapshot,
    /// Explicitly allowed tool identifiers.
    /// 显式允许的工具标识符。
    pub allowed_tools: Vec<String>,
    /// Explicitly disabled tool identifiers.
    /// 显式禁用的工具标识符。
    pub disabled_tools: Vec<String>,
    /// Explicitly allowed AI capability identifiers.
    /// 显式允许的 AI 能力标识符。
    pub allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability identifiers.
    /// 显式禁用的 AI 能力标识符。
    pub disabled_ai_capabilities: Vec<String>,
    /// LuaSkills override payload.
    /// LuaSkills 覆盖载荷。
    pub luaskills_overrides: Value,
    /// MCP server override payload.
    /// MCP 服务覆盖载荷。
    pub mcp_server_overrides: Value,
}

/// Prompt skill snapshot visible to one session node.
/// 单个会话节点可见的提示词技能快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PromptSkillSnapshot {
    /// Unified prompt-skill selector policy.
    /// 统一提示词技能选择器策略。
    #[serde(default)]
    pub policy: CapabilityPolicySnapshot,
    /// Enabled prompt skill identifiers.
    /// 启用的提示词技能标识符。
    pub enabled: Vec<String>,
    /// Disabled prompt skill identifiers.
    /// 禁用的提示词技能标识符。
    pub disabled: Vec<String>,
}

/// Context compaction policy snapshot for one session node.
/// 单个会话节点的上下文压缩策略快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextCompactionPolicySnapshot {
    /// Optional token budget for the context.
    /// 可选的上下文 token 预算。
    pub context_budget: Option<u64>,
    /// Optional maximum provider request count before compaction.
    /// 压缩前允许的最大供应商请求数量。
    pub max_provider_requests: Option<usize>,
    /// Whether automatic compaction is enabled.
    /// 是否启用自动压缩。
    pub auto_compaction_enabled: bool,
}

/// Context compaction policy override before final resolution.
/// 最终解析前的上下文压缩策略覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContextCompactionPolicyRequest {
    /// Optional token budget override for the context.
    /// 可选的上下文 token 预算覆盖。
    pub context_budget: Option<u64>,
    /// Optional maximum provider request count override before compaction.
    /// 压缩前允许的最大供应商请求数量覆盖。
    pub max_provider_requests: Option<usize>,
    /// Optional automatic compaction toggle override.
    /// 可选的自动压缩开关覆盖。
    pub auto_compaction_enabled: Option<bool>,
}

/// User requested configuration before inheritance is resolved.
/// 继承解析前的用户请求配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRequestedConfig {
    /// Whether this node should inherit parent configuration.
    /// 该节点是否继承父级配置。
    pub inherit_parent_config: bool,
    /// Optional subagent profile identifier.
    /// 可选的子代理配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Optional subagent execution mode key.
    /// 可选的子代理执行模式键。
    pub subagent_mode: Option<String>,
    /// Optional replacement system prompt.
    /// 可选的替换系统提示词。
    pub system_prompt: Option<String>,
    /// Optional appended system prompt.
    /// 可选的追加系统提示词。
    pub system_prompt_append: Option<String>,
    /// Provider and model override request.
    /// 供应商与模型覆盖请求。
    pub provider_model: ProviderModelSnapshot,
    /// Tool exposure override request.
    /// 工具可见性覆盖请求。
    pub tool_exposure: ToolExposureSnapshot,
    /// Prompt skill override request.
    /// 提示词技能覆盖请求。
    pub prompt_skills: PromptSkillSnapshot,
    /// Context compaction override request.
    /// 上下文压缩覆盖请求。
    pub context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    pub runtime_policy: ExecutionRuntimePolicyRequest,
    /// Additional caller-owned configuration payload.
    /// 调用方拥有的额外配置载荷。
    pub extra: Value,
}

impl Default for SessionRequestedConfig {
    /// Builds the default requested configuration.
    /// 构建默认请求配置。
    fn default() -> Self {
        Self {
            inherit_parent_config: true,
            subagent_profile_id: None,
            subagent_mode: None,
            system_prompt: None,
            system_prompt_append: None,
            provider_model: ProviderModelSnapshot::default(),
            tool_exposure: ToolExposureSnapshot::default(),
            prompt_skills: PromptSkillSnapshot::default(),
            context_compaction: ContextCompactionPolicyRequest::default(),
            runtime_policy: ExecutionRuntimePolicyRequest::default(),
            extra: Value::Null,
        }
    }
}

/// Fully resolved configuration used by execution and persistence.
/// 执行与持久化使用的完整解析配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionEffectiveConfig {
    /// Whether parent configuration participated in resolution.
    /// 父级配置是否参与了解析。
    pub inherit_parent_config: bool,
    /// Resolved subagent profile identifier.
    /// 已解析的子代理配置标识符。
    pub subagent_profile_id: Option<String>,
    /// Resolved subagent execution mode key.
    /// 已解析的子代理执行模式键。
    pub subagent_mode: Option<String>,
    /// Resolved provider and model snapshot.
    /// 已解析的供应商与模型快照。
    pub provider_model: ProviderModelSnapshot,
    /// Ordered system prompt layers.
    /// 有序系统提示词层。
    pub system_prompt_layers: Vec<String>,
    /// Resolved tool exposure snapshot.
    /// 已解析的工具可见性快照。
    pub tool_exposure: ToolExposureSnapshot,
    /// Resolved prompt skill snapshot.
    /// 已解析的提示词技能快照。
    pub prompt_skills: PromptSkillSnapshot,
    /// Resolved context compaction policy.
    /// 已解析的上下文压缩策略。
    pub context_compaction: ContextCompactionPolicySnapshot,
    /// Additional resolved configuration payload.
    /// 额外的已解析配置载荷。
    pub extra: Value,
}

impl Default for SessionEffectiveConfig {
    /// Builds an empty resolved configuration that still inherits parent state by default.
    /// 构建默认继承父级状态的空解析配置。
    fn default() -> Self {
        Self {
            inherit_parent_config: true,
            subagent_profile_id: None,
            subagent_mode: None,
            provider_model: ProviderModelSnapshot::default(),
            system_prompt_layers: Vec::new(),
            tool_exposure: ToolExposureSnapshot::default(),
            prompt_skills: PromptSkillSnapshot::default(),
            context_compaction: ContextCompactionPolicySnapshot::default(),
            extra: Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityPolicyMode, CapabilityPolicySnapshot, PromptSkillSnapshot, ToolExposureSnapshot,
        apply_prompt_skill_capability_policy, apply_tool_capability_policy,
        capability_policy_from_mode_or_selector_lists, capability_policy_from_selector_lists,
    };

    /// Verifies allow-only selector policies trim entries, dedupe entries, and remove denied duplicates.
    /// 验证仅允许选择器策略会裁剪条目、去重条目并移除同时被禁用的重复项。
    #[test]
    fn capability_policy_from_selector_lists_normalizes_allow_only() {
        let policy = capability_policy_from_selector_lists(
            vec![
                String::from(" shell_command "),
                String::from("shell_command"),
                String::from("todolist"),
            ],
            vec![String::from("TODOLIST")],
        );

        assert_eq!(policy.mode, CapabilityPolicyMode::AllowOnly);
        assert_eq!(policy.selectors, vec![String::from("shell_command")]);
    }

    /// Verifies explicit all mode clears selectors and ignores fallback lists.
    /// 验证显式 all 模式会清空选择器并忽略回退列表。
    #[test]
    fn capability_policy_from_mode_or_selector_lists_prefers_explicit_mode() {
        let policy = capability_policy_from_mode_or_selector_lists(
            Some("all"),
            vec![String::from("shell_command")],
            vec![String::from("todolist")],
            vec![String::from("ask")],
        );

        assert_eq!(policy, CapabilityPolicySnapshot::default());
    }

    /// Verifies applying a tool policy updates the legacy mirror lists consistently.
    /// 验证应用工具策略时会一致更新旧版镜像列表。
    #[test]
    fn apply_tool_capability_policy_updates_mirror_lists() {
        let mut snapshot = ToolExposureSnapshot::default();

        apply_tool_capability_policy(
            &mut snapshot,
            CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::DenyOnly,
                selectors: vec![String::from("ask")],
            },
        );

        assert!(snapshot.allowed_tools.is_empty());
        assert_eq!(snapshot.disabled_tools, vec![String::from("ask")]);
        assert_eq!(snapshot.policy.mode, CapabilityPolicyMode::DenyOnly);
    }

    /// Verifies applying a prompt-skill policy updates the legacy mirror lists consistently.
    /// 验证应用提示词技能策略时会一致更新旧版镜像列表。
    #[test]
    fn apply_prompt_skill_capability_policy_updates_mirror_lists() {
        let mut snapshot = PromptSkillSnapshot::default();

        apply_prompt_skill_capability_policy(
            &mut snapshot,
            CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::AllowOnly,
                selectors: vec![String::from("reviewer")],
            },
        );

        assert_eq!(snapshot.enabled, vec![String::from("reviewer")]);
        assert!(snapshot.disabled.is_empty());
        assert_eq!(snapshot.policy.mode, CapabilityPolicyMode::AllowOnly);
    }
}
