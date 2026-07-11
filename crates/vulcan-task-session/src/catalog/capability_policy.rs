//! Subagent capability-policy sidecar contract and normalization.
//! 子代理能力策略 sidecar 契约与规范化。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vulcan_session_core::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, PromptSkillSnapshot, ToolExposureSnapshot,
};

/// Family-specific tool exposure fields persisted beside one subagent definition.
/// 持久化在子代理定义旁的分族工具可见性字段。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubagentToolExposureSidecar {
    /// Explicitly allowed AI capability identifiers.
    /// 显式允许的 AI 能力标识符。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowed_ai_capabilities: Vec<String>,
    /// Explicitly disabled AI capability identifiers.
    /// 显式禁用的 AI 能力标识符。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    disabled_ai_capabilities: Vec<String>,
    /// LuaSkills-specific exposure override payload.
    /// LuaSkills 专属可见性覆盖载荷。
    #[serde(default, skip_serializing_if = "Value::is_null")]
    luaskills_overrides: Value,
    /// MCP-specific exposure override payload.
    /// MCP 专属可见性覆盖载荷。
    #[serde(default, skip_serializing_if = "Value::is_null")]
    mcp_server_overrides: Value,
}

impl SubagentToolExposureSidecar {
    /// Extracts only family-specific exposure fields from a runtime snapshot.
    /// 仅从运行时快照提取分族可见性字段。
    pub(super) fn from_snapshot(snapshot: &ToolExposureSnapshot) -> Self {
        Self {
            allowed_ai_capabilities: snapshot.allowed_ai_capabilities.clone(),
            disabled_ai_capabilities: snapshot.disabled_ai_capabilities.clone(),
            luaskills_overrides: snapshot.luaskills_overrides.clone(),
            mcp_server_overrides: snapshot.mcp_server_overrides.clone(),
        }
    }

    /// Returns whether this sidecar carries any family-specific exposure override.
    /// 返回当前 sidecar 是否携带任何分族可见性覆盖。
    pub(super) fn has_override(&self) -> bool {
        !self.allowed_ai_capabilities.is_empty()
            || !self.disabled_ai_capabilities.is_empty()
            || !self.luaskills_overrides.is_null()
            || !self.mcp_server_overrides.is_null()
    }

    /// Returns whether this sidecar is empty and can be omitted from YAML.
    /// 返回当前 sidecar 是否为空且可从 YAML 省略。
    pub(super) fn is_empty(&self) -> bool {
        !self.has_override()
    }

    /// Converts the persisted family-specific fields into a runtime snapshot.
    /// 将持久化的分族字段转换为运行时快照。
    pub(super) fn into_snapshot(self) -> ToolExposureSnapshot {
        ToolExposureSnapshot {
            allowed_ai_capabilities: self.allowed_ai_capabilities,
            disabled_ai_capabilities: self.disabled_ai_capabilities,
            luaskills_overrides: self.luaskills_overrides,
            mcp_server_overrides: self.mcp_server_overrides,
            ..ToolExposureSnapshot::default()
        }
    }
}

/// Unified tool policy sidecar.
/// 统一工具策略 sidecar。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentToolPolicySidecar {
    /// Tool policy mode.
    /// 工具策略模式。
    #[serde(default)]
    mode: CapabilityPolicyMode,
    /// Tool selectors used by the mode.
    /// 当前模式使用的工具选择器。
    #[serde(default)]
    tools: Vec<String>,
}

impl SubagentToolPolicySidecar {
    /// Builds a sidecar policy from a normalized snapshot.
    /// 从规范化快照构建 sidecar 策略。
    pub(super) fn from_snapshot(snapshot: &CapabilityPolicySnapshot) -> Option<Self> {
        if snapshot.mode == CapabilityPolicyMode::All && snapshot.selectors.is_empty() {
            return None;
        }
        Some(Self {
            mode: snapshot.mode,
            tools: snapshot.selectors.clone(),
        })
    }

    /// Converts this sidecar policy into the shared snapshot shape.
    /// 将当前 sidecar 策略转换为共享快照结构。
    pub(super) fn into_snapshot(self) -> CapabilityPolicySnapshot {
        CapabilityPolicySnapshot {
            mode: self.mode,
            selectors: self.tools,
        }
    }
}

/// Unified prompt-skill policy sidecar.
/// 统一提示词技能策略 sidecar。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentPromptSkillPolicySidecar {
    /// Prompt skill policy mode.
    /// 提示词技能策略模式。
    #[serde(default)]
    mode: CapabilityPolicyMode,
    /// Prompt skill selectors used by the mode.
    /// 当前模式使用的提示词技能选择器。
    #[serde(default)]
    skills: Vec<String>,
}

impl SubagentPromptSkillPolicySidecar {
    /// Builds a sidecar policy from a normalized snapshot.
    /// 从规范化快照构建 sidecar 策略。
    pub(super) fn from_snapshot(snapshot: &CapabilityPolicySnapshot) -> Option<Self> {
        if snapshot.mode == CapabilityPolicyMode::All && snapshot.selectors.is_empty() {
            return None;
        }
        Some(Self {
            mode: snapshot.mode,
            skills: snapshot.selectors.clone(),
        })
    }

    /// Converts this sidecar policy into the shared snapshot shape.
    /// 将当前 sidecar 策略转换为共享快照结构。
    pub(super) fn into_snapshot(self) -> CapabilityPolicySnapshot {
        CapabilityPolicySnapshot {
            mode: self.mode,
            selectors: self.skills,
        }
    }
}

/// Normalizes one tool exposure snapshot in place.
/// 原地规范化一个工具可见性快照。
pub(super) fn normalize_tool_exposure(snapshot: &mut ToolExposureSnapshot) {
    snapshot.allowed_ai_capabilities =
        normalized_string_list(std::mem::take(&mut snapshot.allowed_ai_capabilities));
    snapshot.disabled_ai_capabilities =
        normalized_string_list(std::mem::take(&mut snapshot.disabled_ai_capabilities));
    // Owns the explicit policy while its runtime mirrors are rebuilt.
    // 在重建运行时镜像期间持有显式策略。
    let policy = std::mem::take(&mut snapshot.policy);
    vulcan_session_core::apply_tool_capability_policy(snapshot, policy);
    vulcan_session_core::sanitize_child_agent_tool_exposure(snapshot);
}

/// Normalizes one prompt skill snapshot in place.
/// 原地规范化一个提示词技能快照。
pub(super) fn normalize_prompt_skills(snapshot: &mut PromptSkillSnapshot) {
    // Owns the explicit policy while its runtime mirrors are rebuilt.
    // 在重建运行时镜像期间持有显式策略。
    let policy = std::mem::take(&mut snapshot.policy);
    vulcan_session_core::apply_prompt_skill_capability_policy(snapshot, policy);
}

/// Normalizes string lists by trimming empties and preserving first occurrence order.
/// 通过裁剪空值并保留首次出现顺序来规范化字符串列表。
fn normalized_string_list(values: Vec<String>) -> Vec<String> {
    // Collects unique normalized values in their original order.
    // 按原始顺序收集唯一的规范化值。
    let mut normalized = Vec::new();
    // Examines every caller-provided value exactly once.
    // 对调用方提供的每个值仅检查一次。
    for value in values {
        // Borrows the trimmed representation before allocating an owned result.
        // 在分配所有权结果前借用裁剪后的表示。
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
