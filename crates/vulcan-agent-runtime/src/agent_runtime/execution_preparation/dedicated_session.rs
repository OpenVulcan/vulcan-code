//! Dedicated-session tool admission applied after ordinary exposure resolution.
//! 在常规可见性解析之后应用的专职会话工具准入。

use super::remove_resolved_tool;
use crate::agent_runtime::execution_models::DedicatedExecutionProfile;
use crate::tooling::collaboration::{
    ASK_PARENT_TOOL_ID, ASK_QUESTION_TOOL_ID, CANCEL_QUESTION_TOOL_ID, LIST_SESSIONS_TOOL_ID,
    READ_SESSION_TOOL_ID, REPLY_QUESTION_TOOL_ID, SEND_NOTIFICATION_TOOL_ID,
    SEND_SESSION_MESSAGE_TOOL_ID, WAIT_AGENT_TOOL_ID, WAIT_AGENTS_RESULT_TOOL_ID,
    WAIT_QUESTION_TOOL_ID, WAIT_SESSION_TOOL_ID,
};
use crate::tooling::native::{
    CREATE_GOAL_TOOL_ID, GET_GOAL_TOOL_ID, SUBMIT_PLAN_TOOL_ID, UPDATE_GOAL_TOOL_ID,
};
use crate::vulcan_tools_bridge::{
    native_tool_id_is_knowledge_tool, native_tool_id_is_vmm_memory_tool,
};
use vulcan_session_core::v1::DedicatedToolAccess;
use vulcan_tools_manager::{ResolvedToolSet, VulcanToolSourceKind};

/// Tools that dedicated sessions never expose even when general tools are open.
/// 即使开放通用工具，专职会话也从不暴露的工具。
const DEDICATED_EXCLUDED_TOOL_IDS: [&str; 16] = [
    ASK_PARENT_TOOL_ID,
    ASK_QUESTION_TOOL_ID,
    CANCEL_QUESTION_TOOL_ID,
    LIST_SESSIONS_TOOL_ID,
    READ_SESSION_TOOL_ID,
    REPLY_QUESTION_TOOL_ID,
    SEND_NOTIFICATION_TOOL_ID,
    SEND_SESSION_MESSAGE_TOOL_ID,
    WAIT_AGENTS_RESULT_TOOL_ID,
    WAIT_AGENT_TOOL_ID,
    WAIT_QUESTION_TOOL_ID,
    WAIT_SESSION_TOOL_ID,
    GET_GOAL_TOOL_ID,
    CREATE_GOAL_TOOL_ID,
    UPDATE_GOAL_TOOL_ID,
    SUBMIT_PLAN_TOOL_ID,
];

/// Returns whether one resolved tool stays visible in a dedicated session.
/// 返回单个已解析工具在专职会话中是否保持可见。
///
/// Knowledge tools are registered only when libraries were enabled at creation, so they stay
/// in both access levels; `none` admits nothing else, `standard` drops collaboration, subagent,
/// goal, plan and memory tools.
/// 知识库工具只在创建时启用了知识库时注册，因此两种方式都保留；`none` 不再允许其他工具，
/// `standard` 移除协作、子代理、目标、计划与记忆工具。
fn tool_allowed(
    tool_access: DedicatedToolAccess,
    tool_id: &str,
    source: VulcanToolSourceKind,
) -> bool {
    if source == VulcanToolSourceKind::Native && native_tool_id_is_knowledge_tool(tool_id) {
        return true;
    }
    match tool_access {
        DedicatedToolAccess::None => false,
        DedicatedToolAccess::Standard => {
            !native_tool_id_is_vmm_memory_tool(tool_id)
                && !vulcan_subagent_core::CORE_SUBAGENT_TOOLS.contains(&tool_id)
                && !DEDICATED_EXCLUDED_TOOL_IDS.contains(&tool_id)
        }
    }
}

/// Removes every tool a dedicated session must not expose from descriptors and definitions.
/// 从描述与定义两侧移除专职会话不得暴露的全部工具。
pub(super) fn filter_tools(
    resolved_tools: &mut ResolvedToolSet,
    profile: Option<&DedicatedExecutionProfile>,
) {
    let Some(profile) = profile else {
        return;
    };
    let rejected = resolved_tools
        .descriptors
        .iter()
        .filter(|descriptor| !tool_allowed(profile.tool_access, &descriptor.id, descriptor.source))
        .map(|descriptor| descriptor.id.clone())
        .collect::<Vec<_>>();
    for tool_id in rejected {
        remove_resolved_tool(resolved_tools, &tool_id);
    }
}

#[cfg(test)]
mod tests {
    use super::tool_allowed;
    use vulcan_session_core::v1::DedicatedToolAccess;
    use vulcan_tools_manager::VulcanToolSourceKind;

    /// No-tools mode keeps only knowledge tools; standard mode keeps ordinary tools and `ask`.
    /// 无工具模式只保留知识库工具；标准模式保留普通工具与 `ask`。
    #[test]
    fn access_levels_admit_expected_tools() {
        let native = VulcanToolSourceKind::Native;
        let managed = VulcanToolSourceKind::VulcanManaged;
        assert!(tool_allowed(
            DedicatedToolAccess::None,
            "knowledge_search",
            native
        ));
        for access in [DedicatedToolAccess::None, DedicatedToolAccess::Standard] {
            for tool in [
                "knowledge_list",
                "knowledge_read",
                "knowledge_search",
                "knowledge_write",
            ] {
                assert!(tool_allowed(access, tool, native));
            }
        }
        assert!(!tool_allowed(DedicatedToolAccess::None, "ask", managed));
        assert!(!tool_allowed(
            DedicatedToolAccess::None,
            "shell_command",
            native
        ));
        assert!(tool_allowed(DedicatedToolAccess::Standard, "ask", managed));
        assert!(tool_allowed(
            DedicatedToolAccess::Standard,
            "shell_command",
            native
        ));
        for denied in [
            "spawn_agent",
            "create_goal",
            "submit_plan",
            "vmm_memory_search",
            "send_session_message",
        ] {
            assert!(
                !tool_allowed(DedicatedToolAccess::Standard, denied, native),
                "`{denied}` must stay hidden in dedicated sessions"
            );
        }
    }
}
