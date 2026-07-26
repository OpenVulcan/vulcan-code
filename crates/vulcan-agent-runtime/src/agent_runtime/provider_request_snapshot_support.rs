//! Shared provider request snapshot support helpers reused by preview rendering, tool schema labeling, and transport snapshots.
//! 供预览渲染、工具 schema 标记与传输快照复用的 Provider 请求快照通用辅助逻辑。

use reqwest::header::HeaderMap;
use serde_json::{Map as JsonMap, Value};
use vulcan_tools_manager::VulcanToolSourceKind;

/// Serializes one request-header map into plain JSON for request-snapshot persistence and inspection.
/// 将一份请求头映射序列化为普通 JSON，供请求快照持久化与检查使用。
pub(crate) fn serialize_provider_request_headers(headers: &HeaderMap) -> Option<Value> {
    let mut object = JsonMap::new();
    for (name, value) in headers {
        if let Ok(value_text) = value.to_str() {
            object.insert(
                name.as_str().to_string(),
                Value::String(value_text.to_string()),
            );
        }
    }

    (!object.is_empty()).then_some(Value::Object(object))
}

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
