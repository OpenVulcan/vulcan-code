//! Shared provider request snapshot support helpers reused by preview rendering, tool schema labeling, and transport snapshots.
//! 供预览渲染、工具 schema 标记与传输快照复用的 Provider 请求快照通用辅助逻辑。

use vulcan_tools_manager::VulcanToolSourceKind;

/// Renders one tool-source label used by request-preview and request-snapshot surfaces.
/// 渲染请求预览与请求快照界面使用的单个工具来源标签。
pub(crate) fn tool_source_label(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "Native tool",
        VulcanToolSourceKind::VulcanManaged => "Vulcan managed tool",
        VulcanToolSourceKind::VulcanPlugin => "Vulcan plugin",
        VulcanToolSourceKind::LuaSkills => "LuaSkills",
        VulcanToolSourceKind::Mcp => "MCP",
        VulcanToolSourceKind::AiHostBinding => "AI-only fallback",
    }
}

/// Returns the stable machine key retained in request measurements.
/// 返回请求测量中保留的稳定机器键。
pub(crate) const fn tool_source_key(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "native",
        VulcanToolSourceKind::VulcanManaged => "vulcan_managed",
        VulcanToolSourceKind::VulcanPlugin => "vulcan_plugin",
        VulcanToolSourceKind::LuaSkills => "luaskills",
        VulcanToolSourceKind::Mcp => "mcp",
        VulcanToolSourceKind::AiHostBinding => "ai_host_binding",
    }
}
