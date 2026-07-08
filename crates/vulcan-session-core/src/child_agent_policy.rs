//! Shared child-agent policy facts used across execution, catalog, and UI projection layers.
//! 执行、目录与 UI 投影层共享的智能体策略事实。

use crate::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ToolExposureSnapshot,
    apply_tool_capability_policy, capability_policy_from_selector_lists,
    normalize_capability_policy_snapshot,
};

/// Parent-owned tool ids that child agents must never execute directly.
/// 智能体绝不能直接执行的父会话拥有工具标识。
pub const CHILD_AGENT_FORBIDDEN_TOOL_IDS: &[&str] = &[
    "spawn_agent",
    "agent_orchestration_info",
    "send_message",
    "wait_agent",
    "interrupt_agent",
    "close_agent",
    "list_agents",
    "ask",
    "submit_plan",
    "todolist",
    "vmm_memory_search",
    "vmm_turn_details",
    "vmm_memory_write",
    "vmm_memory_delete",
];

/// Returns the stable list of parent-owned tools forbidden in child-agent mode.
/// 返回智能体模式下禁用的稳定父会话工具列表。
///
/// # Returns
/// 返回值
/// A static slice of public tool ids that cannot be used by child agents.
/// 智能体不可使用的公开工具标识静态切片。
pub fn child_agent_forbidden_tool_ids() -> &'static [&'static str] {
    CHILD_AGENT_FORBIDDEN_TOOL_IDS
}

/// Returns whether a public tool id is forbidden in child-agent mode.
/// 返回公开工具标识是否在智能体模式下禁用。
///
/// # Parameters
/// 参数
/// `tool_id` is the public tool id or selector to test.
/// `tool_id` 是待检测的公开工具标识或选择器。
///
/// # Returns
/// 返回值
/// `true` when the id matches a parent-owned tool reserved from child agents.
/// 当该标识命中智能体禁用的父会话工具时返回 `true`。
pub fn child_agent_tool_is_forbidden(tool_id: &str) -> bool {
    let normalized = crate::normalize_tool_selector(tool_id);
    normalized.is_some_and(|candidate| {
        CHILD_AGENT_FORBIDDEN_TOOL_IDS.iter().any(|forbidden| {
            crate::normalize_tool_selector(forbidden).as_deref() == Some(candidate.as_str())
        })
    })
}

/// Removes child-agent-forbidden tools from one persisted tool exposure snapshot.
/// 从一份持久化工具可见性快照中移除智能体禁用工具。
///
/// # Parameters
/// 参数
/// `snapshot` is the tool exposure snapshot loaded from or saved to an agent definition.
/// `snapshot` 是从智能体定义读取或写入智能体定义的工具可见性快照。
pub fn sanitize_child_agent_tool_exposure(snapshot: &mut ToolExposureSnapshot) {
    let policy = if snapshot.policy.mode == CapabilityPolicyMode::All
        && snapshot.policy.selectors.is_empty()
    {
        capability_policy_from_selector_lists(
            snapshot.allowed_tools.clone(),
            snapshot.disabled_tools.clone(),
        )
    } else {
        snapshot.policy.clone()
    };
    apply_tool_capability_policy(snapshot, sanitize_child_agent_tool_policy(policy));
}

/// Removes child-agent-forbidden selectors from one unified capability policy.
/// 从一份统一能力策略中移除智能体禁用选择器。
///
/// # Parameters
/// 参数
/// `policy` is the unified capability policy before child-agent policy enforcement.
/// `policy` 是应用智能体策略前的统一能力策略。
///
/// # Returns
/// 返回值
/// A normalized policy that cannot include parent-owned child-agent-forbidden tools.
/// 返回不包含父会话拥有的智能体禁用工具的规范化策略。
pub fn sanitize_child_agent_tool_policy(
    policy: CapabilityPolicySnapshot,
) -> CapabilityPolicySnapshot {
    let mut policy = normalize_capability_policy_snapshot(policy);
    policy
        .selectors
        .retain(|selector| !child_agent_tool_is_forbidden(selector));
    if policy.mode == CapabilityPolicyMode::DenyOnly && policy.selectors.is_empty() {
        return CapabilityPolicySnapshot::default();
    }
    policy
}

#[cfg(test)]
mod tests {
    use super::{
        child_agent_forbidden_tool_ids, child_agent_tool_is_forbidden,
        sanitize_child_agent_tool_exposure,
    };
    use crate::{CapabilityPolicyMode, CapabilityPolicySnapshot, ToolExposureSnapshot};

    /// Verifies every parent-owned interactive tool is forbidden for child agents.
    /// 验证所有父会话拥有的交互工具都禁止智能体调用。
    #[test]
    fn child_agent_forbidden_tools_include_parent_interactive_tools() {
        let forbidden_tools = child_agent_forbidden_tool_ids();
        for tool_id in [
            "spawn_agent",
            "agent_orchestration_info",
            "send_message",
            "wait_agent",
            "interrupt_agent",
            "close_agent",
            "list_agents",
            "ask",
            "submit_plan",
            "todolist",
        ] {
            assert!(
                forbidden_tools.contains(&tool_id),
                "child-agent hard-disabled tools must include {tool_id}"
            );
            assert!(child_agent_tool_is_forbidden(&tool_id.to_ascii_uppercase()));
        }
    }

    /// Verifies memory tools stay owned by the parent runtime.
    /// 验证记忆工具始终归父级运行时所有。
    #[test]
    fn child_agent_forbidden_tools_include_memory_tools() {
        let forbidden_tools = child_agent_forbidden_tool_ids();
        for tool_id in [
            "vmm_memory_search",
            "vmm_turn_details",
            "vmm_memory_write",
            "vmm_memory_delete",
        ] {
            assert!(
                forbidden_tools.contains(&tool_id),
                "child-agent hard-disabled tools must include {tool_id}"
            );
        }
    }

    /// Verifies explicit allow policies cannot persist parent-owned tools for child agents.
    /// 验证显式允许策略不能为智能体持久化父会话拥有工具。
    #[test]
    fn child_agent_tool_exposure_sanitizes_allowed_forbidden_tools() {
        let mut snapshot = ToolExposureSnapshot {
            policy: CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::AllowOnly,
                selectors: vec![
                    String::from("shell_command"),
                    String::from("todolist"),
                    String::from("ASK"),
                ],
            },
            ..ToolExposureSnapshot::default()
        };

        sanitize_child_agent_tool_exposure(&mut snapshot);

        assert_eq!(snapshot.policy.mode, CapabilityPolicyMode::AllowOnly);
        assert_eq!(
            snapshot.policy.selectors,
            vec![String::from("shell_command")]
        );
        assert_eq!(snapshot.allowed_tools, vec![String::from("shell_command")]);
        assert!(snapshot.disabled_tools.is_empty());
    }

    /// Verifies allow-only remains meaningful when every requested tool is forbidden.
    /// 验证请求工具全部被禁用时，仅允许模式仍保持空工具语义。
    #[test]
    fn child_agent_tool_exposure_preserves_empty_allow_only_after_sanitize() {
        let mut snapshot = ToolExposureSnapshot {
            policy: CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::AllowOnly,
                selectors: vec![String::from("todolist")],
            },
            ..ToolExposureSnapshot::default()
        };

        sanitize_child_agent_tool_exposure(&mut snapshot);

        assert_eq!(snapshot.policy.mode, CapabilityPolicyMode::AllowOnly);
        assert!(snapshot.policy.selectors.is_empty());
        assert!(snapshot.allowed_tools.is_empty());
        assert!(snapshot.disabled_tools.is_empty());
    }

    /// Verifies deny-only becomes the default policy when it only denied forbidden tools.
    /// 验证仅禁止策略只包含禁用工具时会折叠为默认策略。
    #[test]
    fn child_agent_tool_exposure_collapses_redundant_forbidden_deny_only() {
        let mut snapshot = ToolExposureSnapshot {
            policy: CapabilityPolicySnapshot {
                mode: CapabilityPolicyMode::DenyOnly,
                selectors: vec![String::from("todolist")],
            },
            ..ToolExposureSnapshot::default()
        };

        sanitize_child_agent_tool_exposure(&mut snapshot);

        assert_eq!(snapshot.policy, CapabilityPolicySnapshot::default());
        assert!(snapshot.allowed_tools.is_empty());
        assert!(snapshot.disabled_tools.is_empty());
    }
}
