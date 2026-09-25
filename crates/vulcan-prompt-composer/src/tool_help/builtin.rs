use crate::tool_help::registry::ToolHelpEntry;

/// Return the built-in tool-help entries.
/// 返回内置工具帮助条目。
pub fn built_in_tool_help_entries() -> Vec<ToolHelpEntry> {
    vec![
        ToolHelpEntry::new(
            "ask",
            "Ask the user one focused clarification only when necessary and prefer concise questions.",
        ),
        ToolHelpEntry::new(
            "shell_command",
            "Use shell_command for concrete local inspection, validation, and deterministic environment checks.",
        ),
        ToolHelpEntry::new(
            "terminal_session",
            "Before using terminal_session, call skill(\"terminal-session\") and follow that skill for persistent terminals, shortcuts, TUI screens, and incremental output.",
        ),
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
            "Continue related work with the existing child instead of spawning a replacement. Queue normal follow-ups; interrupt only when the current work is obsolete and the replacement instruction must run next. If no replacement instruction is ready, use interrupt_agent instead.",
        ),
        ToolHelpEntry::new(
            "wait_agents_result",
            "Use the shared inbox to receive agent questions, answers, notifications and task results without selecting a target or revision. Messages are appended through host-tagged context after the wait. A timeout ends only this wait. no_pending_work means no related live work remains; stopped or abandoned outcomes are terminal, not reasons to wait again. Use read_session or list_agents only for diagnostics, never to reconstruct a result.",
        ),
        ToolHelpEntry::new(
            "interrupt_agent",
            "Choose this when work must stop but no replacement task should be scheduled yet. When the replacement instruction is already known, prefer send_message in interrupt mode so stopping and rescheduling stay one operation.",
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
