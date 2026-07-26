//! JSON Schema generation for the configured web-search tool.
//! 配置化网页搜索工具的 JSON Schema 生成。

use serde_json::{Value, json};

use crate::provider_config::PersistedAiToolConfiguration;

/// Builds the JSON schema used by the configured web-search fallback tool.
/// 构建配置化网页搜索回退工具使用的 JSON Schema。
pub fn configured_web_search_parameters_schema(
    _configuration: &PersistedAiToolConfiguration,
) -> Value {
    json!({
        "type": "object",
        "properties": {
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
