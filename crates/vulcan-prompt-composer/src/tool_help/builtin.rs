use crate::tool_help::registry::ToolHelpEntry;

/// Return the built-in tool-help entries.
/// 返回内置工具帮助条目。
pub fn built_in_tool_help_entries() -> Vec<ToolHelpEntry> {
    vec![
        ToolHelpEntry::new("ask", "Keep each clarification focused and concise."),
        ToolHelpEntry::new(
            "spawn_agent",
            "Delegate only when isolation or parallelism materially helps. Only the root conversation may delegate, and child agents cannot create another orchestration level. Give each child one bounded responsibility and enough context to work independently. Query agent_orchestration_info before choosing a profile or model, then coordinate the child and collect its result with wait_agents_result.",
        ),
        ToolHelpEntry::new(
            "agent_orchestration_info",
            "Call this before delegation when the choice depends on a profile, creation mode, model, or current capacity. Treat returned identifiers as authoritative; do not guess or infer unavailable choices.",
        ),
        ToolHelpEntry::new(
            "send_message",
            "Reuse the existing child for related work. Use interrupt_agent if the active run must stop without new instructions.",
        ),
        ToolHelpEntry::new(
            "interrupt_agent",
            "Use send_message in interrupt mode to steer ongoing work at its next API request.",
        ),
        ToolHelpEntry::new(
            "close_agent",
            "Use this only after required feedback has been collected and no further follow-up is needed. Closing is cleanup, not result collection; wait first whenever the parent task still depends on the child.",
        ),
        ToolHelpEntry::new(
            "list_agents",
            "Use this for diagnostics and status inspection, not synchronization. When the parent task needs a result or message, use wait_agents_result. The listed agents are direct children of the root conversation; nested delegation is unavailable.",
        ),
        ToolHelpEntry::new(
            "task",
            "Use delegated task/subagent execution only when the work benefits from isolated execution or parallel specialist handling.",
        ),
    ]
}
