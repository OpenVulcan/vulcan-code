//! JSON Schema generation for the configured web-search tool.
//! 配置化网页搜索工具的 JSON Schema 生成。

use serde_json::{Value, json};

use crate::provider_config::{AiToolKind, PersistedAiToolConfiguration};

/// Builds the JSON schema used by the configured web-search fallback tool.
/// 构建配置化网页搜索回退工具使用的 JSON Schema。
pub fn configured_web_search_parameters_schema(
    configuration: &PersistedAiToolConfiguration,
) -> Value {
    let target_enum = std::iter::once(String::from("auto"))
        .chain(configuration.usable_targets_for_kind(AiToolKind::WebSearch))
        .collect::<Vec<_>>();
    json!({
        "type": "object",
        "properties": {
            "target": {
                "type": "string",
                "enum": target_enum,
                "default": "auto",
                "description": "Search target selector. Use `auto` to follow the configured default order, or choose one explicit instance/target pair."
            },
            "query": {
                "type": "string",
                "description": "Search query text."
            },
            "max_results": {
                "type": "integer",
                "minimum": 1,
                "maximum": 20,
                "description": "Optional maximum result count."
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}
