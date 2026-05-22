//! Host-owned tool-definition protocol used by provider transport and request bridge layers.
//! 供 Provider 传输与请求桥接层使用的宿主自有工具定义协议。

use serde_json::Value;
use vulcan_tools_manager::ResolvedToolDefinition;

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

/// Recursively normalizes one provider tool-parameters schema by inferring explicit container types for object- and array-shaped branches.
/// 递归规范化单条 Provider 工具参数 schema，并为对象形与数组形分支补齐显式容器类型。
pub(crate) fn normalize_provider_tool_parameters_schema(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if object.contains_key("$ref") {
                return;
            }

            normalize_schema_multi_type_union(object);

            if !object.contains_key("type") {
                if schema_branch_looks_like_object(object) {
                    object.insert(String::from("type"), Value::String(String::from("object")));
                } else if schema_branch_looks_like_array(object) {
                    object.insert(String::from("type"), Value::String(String::from("array")));
                }
            }

            for key in [
                "$defs",
                "definitions",
                "properties",
                "patternProperties",
                "dependentSchemas",
            ] {
                if let Some(Value::Object(entries)) = object.get_mut(key) {
                    for value in entries.values_mut() {
                        normalize_provider_tool_parameters_schema(value);
                    }
                }
            }

            for key in [
                "items",
                "contains",
                "propertyNames",
                "additionalProperties",
                "unevaluatedProperties",
                "unevaluatedItems",
            ] {
                if let Some(value) = object.get_mut(key) {
                    normalize_provider_tool_parameters_schema(value);
                }
            }

            let parent_properties = object.get("properties").cloned();
            let parent_additional_properties = object.get("additionalProperties").cloned();

            for key in ["prefixItems"] {
                if let Some(Value::Array(items)) = object.get_mut(key) {
                    for item in items {
                        normalize_provider_tool_parameters_schema(item);
                    }
                }
            }

            for key in ["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(items)) = object.get_mut(key) {
                    for item in items {
                        normalize_provider_tool_parameters_schema(item);
                        inherit_parent_object_shape_into_combinator_branch(
                            parent_properties.as_ref(),
                            parent_additional_properties.as_ref(),
                            item,
                        );
                    }
                }
            }

            if let Some(branch) = object.get_mut("not") {
                normalize_provider_tool_parameters_schema(branch);
                inherit_parent_object_shape_into_combinator_branch(
                    parent_properties.as_ref(),
                    parent_additional_properties.as_ref(),
                    branch,
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_provider_tool_parameters_schema(item);
            }
        }
        _ => {}
    }
}

/// Rewrites JSON-Schema multi-type arrays into Gemini-compatible forms that use one explicit scalar `type`, optional `nullable`, or `anyOf` branches.
/// 将 JSON Schema 的多类型数组重写为 Gemini 兼容形式，使用单一显式标量 `type`、可选 `nullable` 或 `anyOf` 分支。
fn normalize_schema_multi_type_union(object: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(type_values)) = object.get("type").cloned() else {
        return;
    };

    let mut nullable = object
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut normalized_types = Vec::<String>::new();

    for type_value in type_values {
        let Some(type_name) = type_value.as_str() else {
            return;
        };
        if type_name.eq_ignore_ascii_case("null") {
            nullable = true;
            continue;
        }
        if !normalized_types.iter().any(|existing| existing == type_name) {
            normalized_types.push(type_name.to_string());
        }
    }

    match normalized_types.len() {
        0 => {
            object.insert(String::from("type"), Value::String(String::from("null")));
        }
        1 => {
            object.insert(
                String::from("type"),
                Value::String(normalized_types[0].clone()),
            );
        }
        _ => {
            object.remove("type");
            let mut any_of = normalized_types
                .into_iter()
                .map(|type_name| {
                    Value::Object(serde_json::Map::from_iter([(
                        String::from("type"),
                        Value::String(type_name),
                    )]))
                })
                .collect::<Vec<_>>();
            if let Some(Value::Array(mut existing_any_of)) = object.remove("anyOf") {
                any_of.append(&mut existing_any_of);
            }
            object.insert(String::from("anyOf"), Value::Array(any_of));
        }
    }

    if nullable {
        object.insert(String::from("nullable"), Value::Bool(true));
    }
}

/// Returns whether one schema branch semantically behaves like an object schema and therefore needs an explicit `type: object` for stricter upstream validators.
/// 返回单个 schema 分支是否在语义上表现为对象 schema，从而需要为更严格的上游校验器补齐显式 `type: object`。
fn schema_branch_looks_like_object(object: &serde_json::Map<String, Value>) -> bool {
    [
        "properties",
        "required",
        "additionalProperties",
        "patternProperties",
        "propertyNames",
        "minProperties",
        "maxProperties",
        "dependentRequired",
        "dependentSchemas",
        "unevaluatedProperties",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Returns whether one schema branch semantically behaves like an array schema and therefore needs an explicit `type: array` for stricter upstream validators.
/// 返回单个 schema 分支是否在语义上表现为数组 schema，从而需要为更严格的上游校验器补齐显式 `type: array`。
fn schema_branch_looks_like_array(object: &serde_json::Map<String, Value>) -> bool {
    [
        "items",
        "prefixItems",
        "contains",
        "minItems",
        "maxItems",
        "uniqueItems",
        "unevaluatedItems",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Copies the parent object-shape view into one combinator branch when stricter upstream validators require `required` names to be declared in the same branch.
/// 当更严格的上游校验器要求 `required` 字段名必须在同一分支内声明时，把父级对象形状视图复制到组合分支中。
fn inherit_parent_object_shape_into_combinator_branch(
    parent_properties: Option<&Value>,
    parent_additional_properties: Option<&Value>,
    branch: &mut Value,
) {
    let Some(branch_object) = branch.as_object_mut() else {
        return;
    };
    if branch_object.get("type").and_then(Value::as_str) != Some("object") {
        return;
    }

    if branch_object.contains_key("properties") {
        return;
    }

    if let Some(parent_properties) = parent_properties {
        branch_object.insert(String::from("properties"), parent_properties.clone());
    }
    if !branch_object.contains_key("additionalProperties") {
        if let Some(additional_properties) = parent_additional_properties {
            branch_object.insert(
                String::from("additionalProperties"),
                additional_properties.clone(),
            );
        }
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
        assert_eq!(schema["oneOf"][0]["properties"]["program"]["type"], json!("string"));
        assert_eq!(schema["oneOf"][0]["additionalProperties"], json!(false));
        assert_eq!(schema["oneOf"][1]["type"], json!("object"));
        assert_eq!(schema["oneOf"][1]["properties"]["log"]["type"], json!("string"));
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
