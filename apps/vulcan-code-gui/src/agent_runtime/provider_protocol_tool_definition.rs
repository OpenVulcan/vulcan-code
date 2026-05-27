//! Host-owned tool-definition protocol used by provider transport and request bridge layers.
//! 供 Provider 传输与请求桥接层使用的宿主自有工具定义协议。

use serde_json::Value;
use vulcan_tools_manager::ResolvedToolDefinition;
#[cfg(test)]
use vulcan_tools_manager::normalize_provider_tool_parameters_schema;

/// Provider-visible tool definition preserved inside the host runtime before the final transport-specific conversion step.
/// 在最终传输专用转换步骤之前，保存在宿主运行时内的 Provider 可见工具定义。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderToolDefinition {
    /// Stable tool name exposed to the model.
    /// 暴露给模型的稳定工具名称。
    pub name: String,
    /// Human-readable tool description exposed to the model.
    /// 暴露给模型的人类可读工具描述。
    pub description: String,
    /// JSON Schema describing the tool arguments.
    /// 描述工具参数的 JSON Schema。
    pub parameters: Value,
}

/// Converts one resolved host tool definition into the host-owned provider tool definition shape.
/// 将一条已解析宿主工具定义转换为宿主自有 Provider 工具定义形状。
pub(crate) fn resolved_tool_definition_to_provider_tool_definition(
    definition: &ResolvedToolDefinition,
) -> ProviderToolDefinition {
    ProviderToolDefinition {
        name: definition.descriptor.id.clone(),
        description: definition.descriptor.description.clone(),
        parameters: definition.parameters_schema.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_provider_tool_parameters_schema;
    use serde_json::json;

    /// Verifies provider tool-parameter normalization adds explicit `array` and nested `object` types for schemas that previously relied on implicit JSON Schema inference.
    /// 验证 Provider 工具参数规范化会为先前依赖隐式 JSON Schema 推断的结构补齐显式 `array` 与嵌套 `object` 类型。
    #[test]
    fn normalize_schema_infers_missing_container_types_recursively() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "nodes": {
                    "items": {
                        "properties": {
                            "file": { "type": "string" },
                            "selector": { "type": "string" }
                        },
                        "required": ["file", "selector"]
                    }
                },
                "patches": {
                    "items": {
                        "properties": {
                            "file": { "type": "string" },
                            "replacement": { "type": "string" }
                        }
                    }
                }
            }
        });

        normalize_provider_tool_parameters_schema(&mut schema);

        assert_eq!(schema["properties"]["nodes"]["type"], json!("array"));
        assert_eq!(
            schema["properties"]["nodes"]["items"]["type"],
            json!("object")
        );
        assert_eq!(schema["properties"]["patches"]["type"], json!("array"));
        assert_eq!(
            schema["properties"]["patches"]["items"]["type"],
            json!("object")
        );
    }

    /// Verifies provider tool-parameter normalization adds explicit `object` types for combinator branches that only declare object-only keywords such as `required`.
    /// 验证 Provider 工具参数规范化会为仅声明 `required` 等对象专属关键字的组合分支补齐显式 `object` 类型。
    #[test]
    fn normalize_schema_infers_object_type_for_required_only_anyof_branch() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "basic": {
                    "type": "object",
                    "properties": {
                        "username": { "type": "string" },
                        "password": { "type": "string" }
                    },
                    "anyOf": [
                        {
                            "required": ["username", "password"]
                        }
                    ]
                }
            }
        });

        normalize_provider_tool_parameters_schema(&mut schema);

        assert_eq!(
            schema["properties"]["basic"]["anyOf"][0]["type"],
            json!("object")
        );
    }

    /// Verifies provider tool-parameter normalization copies parent object properties into required-only combinator branches for stricter Gemini/OpenRouter validators.
    /// 验证 Provider 工具参数规范化会把父级对象属性复制到仅含 required 的组合分支中，以兼容更严格的 Gemini/OpenRouter 校验器。
    #[test]
    fn normalize_schema_inherits_parent_properties_for_required_only_anyof_branch() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "basic": {
                    "type": "object",
                    "properties": {
                        "username": { "type": "string" },
                        "password": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "anyOf": [
                        {
                            "required": ["username", "password"]
                        }
                    ]
                }
            }
        });

        normalize_provider_tool_parameters_schema(&mut schema);

        assert_eq!(
            schema["properties"]["basic"]["anyOf"][0]["properties"]["username"]["type"],
            json!("string")
        );
        assert_eq!(
            schema["properties"]["basic"]["anyOf"][0]["additionalProperties"],
            json!(false)
        );
    }

    /// Verifies provider tool-parameter normalization copies parent object properties into required-only `oneOf` branches so root-level mode selectors remain acceptable to stricter Gemini/OpenRouter validators.
    /// 验证 Provider 工具参数规范化会把父级对象属性复制到仅含 required 的 `oneOf` 分支中，从而让根级模式选择器也能通过更严格的 Gemini/OpenRouter 校验。
    #[test]
    fn normalize_schema_inherits_parent_properties_for_required_only_oneof_branch() {
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "program": {
                    "type": "string"
                },
                "log": {
                    "type": "string"
                }
            },
            "oneOf": [
                {
                    "required": ["program"]
                },
                {
                    "required": ["log"]
                }
            ]
        });

        normalize_provider_tool_parameters_schema(&mut schema);

        assert_eq!(schema["oneOf"][0]["type"], json!("object"));
        assert_eq!(
            schema["oneOf"][0]["properties"]["program"]["type"],
            json!("string")
        );
        assert_eq!(schema["oneOf"][0]["additionalProperties"], json!(false));
        assert_eq!(schema["oneOf"][1]["type"], json!("object"));
        assert_eq!(
            schema["oneOf"][1]["properties"]["log"]["type"],
            json!("string")
        );
        assert_eq!(schema["oneOf"][1]["additionalProperties"], json!(false));
    }

    /// Verifies provider tool-parameter normalization rewrites JSON-Schema multi-type arrays into Gemini-compatible `anyOf` branches so scalar union leaves remain valid inside array items and additional-properties nodes.
    /// 验证 Provider 工具参数规范化会把 JSON Schema 的多类型数组重写为 Gemini 兼容的 `anyOf` 分支，从而让数组元素与附加属性中的标量联合类型保持有效。
    #[test]
    fn normalize_schema_rewrites_multi_type_array_into_anyof_branches() {
        let mut schema = json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": {
                        "type": ["string", "number", "boolean"],
                        "description": "One scalar argument item."
                    }
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": {
                        "type": ["string", "number", "boolean"]
                    }
                }
            }
        });

        normalize_provider_tool_parameters_schema(&mut schema);

        assert!(schema["properties"]["args"]["items"]["type"].is_null());
        assert_eq!(
            schema["properties"]["args"]["items"]["anyOf"],
            json!([
                { "type": "string" },
                { "type": "number" },
                { "type": "boolean" }
            ])
        );
        assert_eq!(
            schema["properties"]["headers"]["additionalProperties"]["anyOf"],
            json!([
                { "type": "string" },
                { "type": "number" },
                { "type": "boolean" }
            ])
        );
    }
}
