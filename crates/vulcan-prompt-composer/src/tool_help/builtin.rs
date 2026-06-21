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
            "task",
            "Use delegated task/subagent execution only when the work benefits from isolated execution or parallel specialist handling.",
        ),
    ]
}
