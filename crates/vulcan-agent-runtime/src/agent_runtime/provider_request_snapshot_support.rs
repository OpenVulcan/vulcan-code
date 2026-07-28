//! Shared provider request snapshot support helpers reused by preview rendering, tool schema labeling, and transport snapshots.
//! 供预览渲染、工具 schema 标记与传输快照复用的 Provider 请求快照通用辅助逻辑。

use vulcan_tools_manager::VulcanToolSourceKind;

/// Renders one tool-source label used by request-preview and request-snapshot surfaces.
/// 渲染请求预览与请求快照界面使用的单个工具来源标签。
pub(crate) fn tool_source_label(source: VulcanToolSourceKind) -> &'static str {
    match source {
        VulcanToolSourceKind::Native => "原生工具",
        VulcanToolSourceKind::VulcanManaged => "Vulcan 托管工具",
        VulcanToolSourceKind::VulcanPlugin => "Vulcan 插件",
        VulcanToolSourceKind::LuaSkills => "LuaSkills",
        VulcanToolSourceKind::Mcp => "MCP",
        VulcanToolSourceKind::AiHostBinding => "AI 专用回退",
    }
}
